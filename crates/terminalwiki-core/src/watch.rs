//! Debounced cross-platform filesystem watcher for wiki trees (spec §65-§68).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use notify::{
    Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    Create,
    Modify,
    Delete,
    Rename,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiChangeEvent {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

pub struct WikiWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
    last_event: Option<(PathBuf, Instant)>,
    debounce_duration: Duration,
}

impl WikiWatcher {
    /// Creates a new watcher for a wiki root directory with a default 250ms debounce.
    pub fn new(root: &Path) -> Result<Self> {
        let (tx, rx) = channel();

        let mut watcher = RecommendedWatcher::new(tx, NotifyConfig::default())
            .map_err(|e| Error::other(format!("Failed to initialize file watcher: {e}")))?;

        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| Error::io(root, std::io::Error::other(e.to_string())))?;

        Ok(Self {
            _watcher: watcher,
            rx,
            last_event: None,
            debounce_duration: Duration::from_millis(250),
        })
    }

    /// Polls for the next debounced file change event without blocking.
    pub fn poll_change(&mut self) -> Option<WikiChangeEvent> {
        while let Ok(res) = self.rx.try_recv() {
            if let Ok(event) = res {
                let kind = match event.kind {
                    EventKind::Create(_) => ChangeKind::Create,
                    EventKind::Modify(_) => ChangeKind::Modify,
                    EventKind::Remove(_) => ChangeKind::Delete,
                    _ => continue,
                };

                for path in event.paths {
                    // Ignore internal git or cache files
                    let path_str = path.to_string_lossy();
                    if path_str.contains("/.git/") || path_str.contains("/.cache/") {
                        continue;
                    }

                    let now = Instant::now();
                    if let Some((ref last_path, ref last_time)) = self.last_event {
                        if last_path == &path
                            && now.duration_since(*last_time) < self.debounce_duration
                        {
                            continue;
                        }
                    }

                    self.last_event = Some((path.clone(), now));
                    return Some(WikiChangeEvent { path, kind });
                }
            }
        }
        None
    }
}
