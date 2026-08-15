//! Debounced, coalescing filesystem watcher for wiki trees (spec Gate 2).
//!
//! The wiki directory is the source of truth and users edit it with ordinary
//! tools, so a running TUI has to notice changes made by `nvim`, `cp`, `mv`,
//! `git pull`, `rsync` and anything else.
//!
//! Three properties matter more than raw event fidelity:
//!
//! * **Per-path debouncing.** A single global "last event" timestamp drops
//!   unrelated changes that happen to arrive close together, which is exactly
//!   what a bulk `git checkout` produces.
//! * **Coalescing.** Editors save atomically — write a temp file, rename it
//!   over the target — and emit several raw events per logical save. Consumers
//!   want one change per path.
//! * **Batching.** A `git pull` touching 500 files must produce one batch, not
//!   500 index updates and 500 redraws.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::event::{ModifyKind, RenameMode};
use notify::{
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};

use crate::error::{Error, Result};

/// How long to wait for a path to settle before reporting it.
///
/// Long enough to absorb an editor's write/rename burst, short enough that a
/// save feels immediate.
pub const DEFAULT_COALESCE_WINDOW: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    Create,
    Modify,
    Delete,
    /// A rename whose source was observed. `path` on the [`WikiChange`] is the
    /// destination.
    Rename {
        from: PathBuf,
    },
}

/// One settled, logical change to a watched wiki.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiChange {
    /// Name of the wiki this path belongs to.
    pub wiki: String,
    /// Absolute path. For a rename this is the destination.
    pub path: PathBuf,
    pub kind: ChangeKind,
}

impl WikiChange {
    /// Path relative to the wiki root it was matched against, if it is inside.
    pub fn relative_to(&self, root: &Path) -> Option<PathBuf> {
        self.path.strip_prefix(root).ok().map(|p| p.to_path_buf())
    }
}

/// A change observed but not yet reported, waiting for its path to settle.
#[derive(Debug, Clone)]
struct PendingChange {
    kind: ChangeKind,
    wiki: String,
    last_seen: Instant,
}

struct WatchedRoot {
    name: String,
    root: PathBuf,
    ignore: Gitignore,
}

pub struct WikiWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
    roots: Vec<WatchedRoot>,
    /// One entry per path, so unrelated concurrent changes never mask each
    /// other (spec Gate 2.6).
    pending: HashMap<PathBuf, PendingChange>,
    /// An unpaired rename source, waiting for its destination event.
    pending_rename_from: Option<(PathBuf, Instant)>,
    window: Duration,
}

impl WikiWatcher {
    /// Watches every given `(wiki name, root)` pair with one shared watcher.
    ///
    /// A single watcher and receiver keeps the consumer's event loop simple;
    /// one watcher per wiki would mean one channel per wiki to multiplex.
    pub fn new<I, S>(roots: I) -> Result<Self>
    where
        I: IntoIterator<Item = (S, PathBuf)>,
        S: Into<String>,
    {
        Self::with_window(roots, DEFAULT_COALESCE_WINDOW)
    }

    pub fn with_window<I, S>(roots: I, window: Duration) -> Result<Self>
    where
        I: IntoIterator<Item = (S, PathBuf)>,
        S: Into<String>,
    {
        let (tx, rx) = channel();
        let mut watcher = RecommendedWatcher::new(tx, NotifyConfig::default())
            .map_err(|e| Error::other(format!("Failed to initialize file watcher: {e}")))?;

        let mut watched = Vec::new();
        for (name, root) in roots {
            watcher
                .watch(&root, RecursiveMode::Recursive)
                .map_err(|e| Error::io(&root, std::io::Error::other(e.to_string())))?;
            let ignore = build_ignore(&root);
            // Platform backends report canonical paths — macOS FSEvents turns
            // /var/... into /private/var/... — so the root must be canonical
            // too or `strip_prefix` never matches and every event is dropped.
            let root = root.canonicalize().unwrap_or(root);
            watched.push(WatchedRoot {
                name: name.into(),
                root,
                ignore,
            });
        }

        Ok(Self {
            _watcher: watcher,
            rx,
            roots: watched,
            pending: HashMap::new(),
            pending_rename_from: None,
            window,
        })
    }

    /// Number of wikis being watched.
    pub fn watched_count(&self) -> usize {
        self.roots.len()
    }

    /// Drains raw events and returns every change whose path has settled.
    ///
    /// Non-blocking. Returns an empty vec when nothing has settled yet, so a
    /// caller can poll it alongside other event sources.
    pub fn poll_batch(&mut self) -> Vec<WikiChange> {
        self.drain_raw_events();
        self.take_settled(Instant::now())
    }

    /// Test seam: settle everything regardless of elapsed time.
    pub fn drain_all(&mut self) -> Vec<WikiChange> {
        self.drain_raw_events();
        let mut out: Vec<WikiChange> = self
            .pending
            .drain()
            .map(|(path, p)| WikiChange {
                wiki: p.wiki,
                path,
                kind: p.kind,
            })
            .collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    fn drain_raw_events(&mut self) {
        while let Ok(Ok(event)) = self.rx.try_recv() {
            self.absorb(event);
        }
    }

    fn absorb(&mut self, event: Event) {
        let now = Instant::now();

        match event.kind {
            // Both halves in one event: the clearest rename signal.
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                if event.paths.len() >= 2 {
                    let from = event.paths[0].clone();
                    let to = event.paths[1].clone();
                    self.record_rename(from, to, now);
                    return;
                }
            }
            // Split rename: remember the source until the destination arrives.
            EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                if let Some(p) = event.paths.first() {
                    self.pending_rename_from = Some((p.clone(), now));
                }
                return;
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::To)) => {
                if let Some(to) = event.paths.first().cloned() {
                    // Only pair with a source seen recently; otherwise this is
                    // an ordinary create (e.g. a file moved in from outside).
                    let recent = self
                        .pending_rename_from
                        .as_ref()
                        .filter(|(_, t)| now.duration_since(*t) < self.window * 4)
                        .map(|(p, _)| p.clone());
                    match recent {
                        Some(from) => {
                            self.pending_rename_from = None;
                            self.record_rename(from, to, now);
                        }
                        None => self.record(to, ChangeKind::Create, now),
                    }
                }
                return;
            }
            // macOS FSEvents does not distinguish the halves of a rename, and
            // reports removals the same way. Existence at the time of handling
            // tells the two apart: a path that is gone is a source (or a
            // delete), a path that is present is a destination.
            EventKind::Modify(ModifyKind::Name(RenameMode::Any)) => {
                for path in event.paths {
                    if path.exists() {
                        let recent = self
                            .pending_rename_from
                            .as_ref()
                            .filter(|(_, t)| now.duration_since(*t) < self.window * 4)
                            .map(|(p, _)| p.clone());
                        match recent {
                            Some(from) => {
                                self.pending_rename_from = None;
                                // The source was provisionally recorded as a
                                // delete; the rename supersedes it.
                                self.pending.remove(&from);
                                self.record_rename(from, path, now);
                            }
                            None => self.record(path, ChangeKind::Create, now),
                        }
                    } else {
                        // Provisionally a delete. If a destination shows up
                        // within the window, it is upgraded to a rename.
                        self.pending_rename_from = Some((path.clone(), now));
                        self.record(path, ChangeKind::Delete, now);
                    }
                }
                return;
            }
            _ => {}
        }

        let kind = match event.kind {
            EventKind::Create(_) => ChangeKind::Create,
            EventKind::Modify(_) => ChangeKind::Modify,
            EventKind::Remove(_) => ChangeKind::Delete,
            _ => return,
        };

        for path in event.paths {
            self.record(path, kind.clone(), now);
        }
    }

    fn record_rename(&mut self, from: PathBuf, to: PathBuf, now: Instant) {
        // An atomic save (`.foo.md.tmp` → `foo.md`) is a rename whose source is
        // an ignored temp file. Report it as a plain modification of the target
        // rather than a rename from a path the consumer never knew about.
        let from_tracked = self.match_root(&from).is_some();
        if from_tracked {
            self.record(to, ChangeKind::Rename { from }, now);
        } else {
            self.record(to, ChangeKind::Modify, now);
        }
    }

    fn record(&mut self, path: PathBuf, kind: ChangeKind, now: Instant) {
        let Some(wiki) = self.match_root(&path) else {
            return;
        };

        match self.pending.get_mut(&path) {
            Some(existing) => {
                existing.kind = merge_kind(&existing.kind, &kind);
                existing.last_seen = now;
            }
            None => {
                self.pending.insert(
                    path,
                    PendingChange {
                        kind,
                        wiki,
                        last_seen: now,
                    },
                );
            }
        }
    }

    /// Returns the wiki name owning `path`, or `None` if it is outside every
    /// root or excluded by ignore rules.
    fn match_root(&self, path: &Path) -> Option<String> {
        for root in &self.roots {
            let Ok(rel) = path.strip_prefix(&root.root) else {
                continue;
            };
            if is_vcs_internal(rel) {
                return None;
            }
            // `is_dir` is unreliable for deleted paths; treat as a file, which
            // is the conservative choice for ignore matching.
            if root
                .ignore
                .matched_path_or_any_parents(rel, false)
                .is_ignore()
            {
                return None;
            }
            return Some(root.name.clone());
        }
        None
    }

    fn take_settled(&mut self, now: Instant) -> Vec<WikiChange> {
        let settled: Vec<PathBuf> = self
            .pending
            .iter()
            .filter(|(_, p)| now.duration_since(p.last_seen) >= self.window)
            .map(|(path, _)| path.clone())
            .collect();

        let mut out = Vec::with_capacity(settled.len());
        for path in settled {
            if let Some(p) = self.pending.remove(&path) {
                out.push(WikiChange {
                    wiki: p.wiki,
                    path,
                    kind: p.kind,
                });
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }
}

/// Collapses two observations of the same path into one logical change.
fn merge_kind(existing: &ChangeKind, incoming: &ChangeKind) -> ChangeKind {
    match (existing, incoming) {
        // Created then written: still a creation as far as consumers care.
        (ChangeKind::Create, ChangeKind::Modify) => ChangeKind::Create,
        // Created then removed within one window: report the removal; the
        // consumer re-scans the path and finds nothing, which is correct.
        (ChangeKind::Create, ChangeKind::Delete) => ChangeKind::Delete,
        // A rename already carries the strongest information about the path.
        (ChangeKind::Rename { from }, ChangeKind::Modify) => {
            ChangeKind::Rename { from: from.clone() }
        }
        // Removed then recreated is an atomic-save pattern: a modification.
        (ChangeKind::Delete, ChangeKind::Create) => ChangeKind::Modify,
        // A file cannot be modified after it is gone. Backends coalesce flags
        // for a path (FSEvents sets created|modified|removed together), so a
        // trailing Modify after a Delete is stale and must not resurrect it.
        (ChangeKind::Delete, ChangeKind::Modify) => ChangeKind::Delete,
        _ => incoming.clone(),
    }
}

/// True for paths inside a VCS metadata directory.
///
/// Matched on path *components*, not substrings: a user directory legitimately
/// named `.gitlab` or `cache` must not be skipped.
fn is_vcs_internal(rel: &Path) -> bool {
    rel.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some(".git") | Some(".hg") | Some(".svn") | Some(".jj")
        )
    })
}

/// Builds the ignore matcher for a wiki root from `.twignore` and `.gitignore`.
fn build_ignore(root: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    // `.twignore` is added last so it can re-include what `.gitignore` excluded.
    let _ = builder.add(root.join(".gitignore"));
    let _ = builder.add(root.join(".twignore"));
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ignore_for(root: &Path) -> Gitignore {
        build_ignore(root)
    }

    #[test]
    fn vcs_internal_matches_components_not_substrings() {
        assert!(is_vcs_internal(Path::new(".git/config")));
        assert!(is_vcs_internal(Path::new("nested/.git/HEAD")));
        // A user's own notes about git must not be skipped.
        assert!(!is_vcs_internal(Path::new("notes/git-workflow.md")));
        assert!(!is_vcs_internal(Path::new(".gitlab/ci.md")));
        assert!(!is_vcs_internal(Path::new("cache/notes.md")));
    }

    #[test]
    fn merge_collapses_atomic_save_into_one_change() {
        // remove + recreate (atomic save) is a modification
        assert_eq!(
            merge_kind(&ChangeKind::Delete, &ChangeKind::Create),
            ChangeKind::Modify
        );
        // create + write is still a create
        assert_eq!(
            merge_kind(&ChangeKind::Create, &ChangeKind::Modify),
            ChangeKind::Create
        );
        // create + delete within a window is a delete
        assert_eq!(
            merge_kind(&ChangeKind::Create, &ChangeKind::Delete),
            ChangeKind::Delete
        );
        // a stale Modify must never resurrect a deleted path
        assert_eq!(
            merge_kind(&ChangeKind::Delete, &ChangeKind::Modify),
            ChangeKind::Delete
        );
    }

    #[test]
    fn rename_survives_a_later_modify() {
        let from = PathBuf::from("/w/Old.md");
        let merged = merge_kind(
            &ChangeKind::Rename { from: from.clone() },
            &ChangeKind::Modify,
        );
        assert_eq!(merged, ChangeKind::Rename { from });
    }

    #[test]
    fn gitignore_rules_are_honoured() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join(".gitignore"), "*.log\nbuild/\n").expect("write");
        let ig = ignore_for(root);

        assert!(ig
            .matched_path_or_any_parents(Path::new("debug.log"), false)
            .is_ignore());
        assert!(ig
            .matched_path_or_any_parents(Path::new("build/out.md"), false)
            .is_ignore());
        assert!(!ig
            .matched_path_or_any_parents(Path::new("Heap.md"), false)
            .is_ignore());
    }
}
