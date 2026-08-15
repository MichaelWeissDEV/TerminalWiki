//! TerminalWiki Search and Incremental Indexing Crate (Gate 2).

pub mod backlinks;
pub mod entry;
pub mod fuzzy;
pub mod indexer;
pub mod query;
pub mod store;
pub mod tantivy_store;

use std::collections::BTreeMap;
use std::path::Path;

use terminalwiki_core::config::Config;
use terminalwiki_core::error::{Error, Result};
use terminalwiki_core::paths;
use terminalwiki_core::wiki::Wiki;

pub use backlinks::BacklinkResult;
pub use entry::{document_id, DocumentState, IndexDelta, IndexEntry, SkipReason, SkippedFile};
pub use fuzzy::{FuzzyDataset, FuzzyHit, FuzzyItem};
pub use query::{Query, QueryTerm};
pub use store::{IndexState, StoredState};
pub use tantivy_store::{IndexMeta, SearchHit, TantivyStore, INDEX_SCHEMA_VERSION};

/// Why the index had to be rebuilt from scratch rather than updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebuildReason {
    /// No index existed for this wiki yet.
    NoIndex,
    /// Persisted state was written by a different index schema.
    SchemaChanged { found: u32, expected: u32 },
    /// Persisted state could not be parsed.
    StateUnreadable,
}

impl std::fmt::Display for RebuildReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RebuildReason::NoIndex => write!(f, "no index yet"),
            RebuildReason::SchemaChanged { found, expected } => write!(
                f,
                "search index schema changed (found v{found}, expected v{expected})"
            ),
            RebuildReason::StateUnreadable => write!(f, "index state was unreadable"),
        }
    }
}

/// What [`WikiIndex::open`] did to bring the index in line with the files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reconciliation {
    /// Files and index already agreed; nothing was written.
    AlreadyCurrent,
    /// An incremental delta was applied.
    Updated { changed: usize },
    /// The index was built from scratch.
    Rebuilt(RebuildReason),
    /// Another process holds the writer lock; the existing index was used.
    SkippedIndexBusy,
    /// `index.auto_update = false`; the index was used exactly as stored.
    AutoUpdateDisabled,
}

impl Reconciliation {
    /// A message worth showing the user, if any. Routine no-ops stay silent.
    pub fn user_message(&self) -> Option<String> {
        match self {
            Reconciliation::AlreadyCurrent | Reconciliation::AutoUpdateDisabled => None,
            Reconciliation::Updated { .. } => None,
            // The "rebuilding..." notice is emitted before the work starts by
            // `open_with`; nothing more is needed once it has finished.
            Reconciliation::Rebuilt(_) => None,
            Reconciliation::SkippedIndexBusy => Some(
                "Index is being updated by another process; showing current index.".to_string(),
            ),
        }
    }
}

/// High-level facade for a wiki's search index.
pub struct WikiIndex {
    pub wiki_name: String,
    pub entries: Vec<DocumentState>,
    /// Files seen during this pass that could not be indexed. Callers are
    /// expected to report these; an empty vec means every file was readable.
    pub skipped: Vec<SkippedFile>,
}

impl WikiIndex {
    /// Loads the index metadata for the given wiki from disk.
    pub fn load(wiki_name: &str) -> Result<WikiIndex> {
        let dir = paths::index_dir_for(wiki_name).ok_or_else(|| Error::Config {
            path: None,
            message: "Cannot determine index directory".into(),
        })?;
        let entries = store::load_index(&dir)?.unwrap_or_default();
        Ok(WikiIndex {
            wiki_name: wiki_name.to_string(),
            entries,
            skipped: Vec::new(),
        })
    }

    /// Builds a fresh index by scanning the wiki and rebuilding Tantivy from scratch (spec §15).
    pub fn build(wiki: &Wiki, config: &Config) -> Result<WikiIndex> {
        let dir = paths::index_dir_for(&wiki.name).ok_or_else(|| Error::Config {
            path: None,
            message: "Cannot determine index directory".into(),
        })?;

        let raw_entries = indexer::build_all(wiki, config)?;

        // Rebuild Tantivy index
        let mut tantivy_store = TantivyStore::open_or_create(&dir)?;
        tantivy_store.rebuild_all(&raw_entries)?;

        // Save slim state metadata
        let states: Vec<DocumentState> = raw_entries.into_iter().map(|e| e.to_state()).collect();
        store::save_state(&dir, &states)?;

        Ok(WikiIndex {
            wiki_name: wiki.name.clone(),
            entries: states,
            skipped: Vec::new(),
        })
    }

    /// Incrementally updates the existing index using a Delta model (spec §11, §13, §14).
    pub fn update(wiki: &Wiki, config: &Config) -> Result<WikiIndex> {
        let dir = paths::index_dir_for(&wiki.name).ok_or_else(|| Error::Config {
            path: None,
            message: "Cannot determine index directory".into(),
        })?;

        let existing = store::load_index(&dir)?.unwrap_or_default();
        let delta = indexer::compute_delta(wiki, config, &existing)?;
        let skipped = delta.skipped.clone();

        // Apply delta to Tantivy
        let mut tantivy_store = TantivyStore::open_or_create(&dir)?;
        tantivy_store.apply_delta(&delta)?;

        // Form new states
        let mut new_states = delta.unchanged;
        for item in delta.added {
            new_states.push(item.to_state());
        }
        for item in delta.modified {
            new_states.push(item.to_state());
        }
        new_states.sort_by(|a, b| a.relative.cmp(&b.relative));

        store::save_state(&dir, &new_states)?;

        Ok(WikiIndex {
            wiki_name: wiki.name.clone(),
            entries: new_states,
            skipped,
        })
    }

    /// Loads the index, first reconciling it with what is actually on disk.
    ///
    /// This is the entry point for ordinary read commands. The wiki directory is
    /// the source of truth, so a file created with any external tool must be
    /// visible without the user running `tw index update` first.
    ///
    /// The reconcile is cheap when nothing changed: `compute_delta` stats each
    /// file and compares mtime/size against persisted state, reading content
    /// only for files whose metadata moved. Searching itself remains strictly
    /// read-only — reconciliation is a separate step performed *before* the
    /// search, never something the search does.
    ///
    /// Returns the index plus a report of what reconciliation did, so callers
    /// can tell the user about a rebuild or a skipped update.
    pub fn open(wiki: &Wiki, config: &Config) -> Result<(WikiIndex, Reconciliation)> {
        Self::open_with(wiki, config, |_| {})
    }

    /// [`WikiIndex::open`], notifying before work that may take a while.
    ///
    /// A rebuild of a large wiki takes minutes, so the user must be told before
    /// it starts, not after it finishes (spec Gate 1.7). The notifier exists so
    /// the message can be emitted mid-operation without this crate deciding
    /// where it goes.
    pub fn open_with<F: FnMut(&str)>(
        wiki: &Wiki,
        config: &Config,
        mut notify: F,
    ) -> Result<(WikiIndex, Reconciliation)> {
        let dir = paths::index_dir_for(&wiki.name).ok_or_else(|| Error::Config {
            path: None,
            message: "Cannot determine index directory".into(),
        })?;

        let stored = store::load_state(&dir)?;

        // A schema change is a rebuild, and must be announced rather than
        // laundered through an "incremental" update of every document.
        let rebuild_reason = match &stored {
            store::StoredState::Absent => Some(RebuildReason::NoIndex),
            store::StoredState::SchemaMismatch { found, expected } => {
                Some(RebuildReason::SchemaChanged {
                    found: *found,
                    expected: *expected,
                })
            }
            store::StoredState::Unreadable => Some(RebuildReason::StateUnreadable),
            store::StoredState::Loaded(_) => None,
        };

        if let Some(reason) = rebuild_reason {
            if !config.index.auto_update {
                // Without an index and without permission to build one, report
                // an empty view rather than silently doing minutes of work.
                return Ok((
                    WikiIndex {
                        wiki_name: wiki.name.clone(),
                        entries: Vec::new(),
                        skipped: Vec::new(),
                    },
                    Reconciliation::AutoUpdateDisabled,
                ));
            }
            // Announce before the work, not after it.
            if reason != RebuildReason::NoIndex {
                notify(&format!("{reason}. Rebuilding index..."));
            }
            let index = Self::build(wiki, config)?;
            return Ok((index, Reconciliation::Rebuilt(reason)));
        }

        let existing = match stored {
            store::StoredState::Loaded(entries) => entries,
            _ => unreachable!("rebuild reasons handled above"),
        };

        if !config.index.auto_update {
            return Ok((
                WikiIndex {
                    wiki_name: wiki.name.clone(),
                    entries: existing,
                    skipped: Vec::new(),
                },
                Reconciliation::AutoUpdateDisabled,
            ));
        }

        let delta = indexer::compute_delta(wiki, config, &existing)?;
        let changed = delta.added.len() + delta.modified.len() + delta.deleted_doc_ids.len();

        if changed == 0 && delta.skipped.is_empty() {
            return Ok((
                WikiIndex {
                    wiki_name: wiki.name.clone(),
                    entries: existing,
                    skipped: Vec::new(),
                },
                Reconciliation::AlreadyCurrent,
            ));
        }

        let skipped = delta.skipped.clone();
        let mut tantivy_store = TantivyStore::open_or_create(&dir)?;

        // Another process may be updating the same index. Skipping is correct:
        // the caller reads a slightly stale but valid index instead of failing.
        if !tantivy_store.try_apply_delta(&delta)? {
            return Ok((
                WikiIndex {
                    wiki_name: wiki.name.clone(),
                    entries: existing,
                    skipped,
                },
                Reconciliation::SkippedIndexBusy,
            ));
        }

        let mut new_states = delta.unchanged;
        for item in delta.added {
            new_states.push(item.to_state());
        }
        for item in delta.modified {
            new_states.push(item.to_state());
        }
        new_states.sort_by(|a, b| a.relative.cmp(&b.relative));
        store::save_state(&dir, &new_states)?;

        Ok((
            WikiIndex {
                wiki_name: wiki.name.clone(),
                entries: new_states,
                skipped,
            },
            Reconciliation::Updated { changed },
        ))
    }

    /// Searches the persistent Tantivy index strictly read-only without destructive mutations (spec §20).
    pub fn search(&self, query: &Query) -> Result<Vec<SearchHit>> {
        let dir = paths::index_dir_for(&self.wiki_name).ok_or_else(|| Error::Config {
            path: None,
            message: "Cannot determine index directory".into(),
        })?;

        let store = TantivyStore::open_reader(&dir)?;
        store.search(query, 50)
    }

    /// Performs instant fuzzy search across page titles, paths, aliases, and tags using Nucleo.
    pub fn find(&self, needle: &str, limit: usize) -> Vec<FuzzyHit> {
        let items: Vec<FuzzyItem> = self
            .entries
            .iter()
            .map(|e| FuzzyItem {
                wiki: e.wiki.clone(),
                relative: e.relative.clone(),
                title: e.title.clone(),
                aliases: e.aliases.clone(),
                tags: e.tags.clone(),
            })
            .collect();

        let mut dataset = FuzzyDataset::new(items);
        dataset.find(needle, limit)
    }

    /// Finds all pages that link to the given relative path.
    pub fn backlinks(&self, page_relative: &Path) -> Vec<BacklinkResult> {
        let mut results = Vec::new();
        let target_stem = page_relative
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let target_rel = page_relative.to_string_lossy().to_string();

        for entry in &self.entries {
            // One result per linking page, even if it links several times.
            if entry
                .wiki_links
                .iter()
                .any(|link| link == &target_stem || link == &target_rel)
            {
                results.push(BacklinkResult {
                    from: entry.relative.clone(),
                    context: format!("Page: {}", entry.title),
                });
            }
        }
        results
    }

    /// Returns a map of all tags and the pages that use them.
    pub fn tags(&self) -> BTreeMap<String, Vec<String>> {
        let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for entry in &self.entries {
            for tag in &entry.tags {
                map.entry(tag.clone())
                    .or_default()
                    .push(entry.relative.to_string_lossy().to_string());
            }
        }
        map
    }
}
