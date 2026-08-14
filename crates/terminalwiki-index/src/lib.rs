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
pub use entry::{document_id, DocumentState, IndexDelta, IndexEntry};
pub use fuzzy::{FuzzyDataset, FuzzyHit, FuzzyItem};
pub use query::{Query, QueryTerm};
pub use store::IndexState;
pub use tantivy_store::{IndexMeta, SearchHit, TantivyStore, INDEX_SCHEMA_VERSION};

/// High-level facade for a wiki's search index.
pub struct WikiIndex {
    pub wiki_name: String,
    pub entries: Vec<DocumentState>,
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
        })
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
            for link in &entry.wiki_links {
                if link == &target_stem || link == &target_rel {
                    results.push(BacklinkResult {
                        from: entry.relative.clone(),
                        context: format!("Page: {}", entry.title),
                    });
                }
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
