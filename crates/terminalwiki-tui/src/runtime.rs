//! Central event runtime for the TUI (spec Gate 2.1-2.3).
//!
//! The loop has to serve several independent sources — the keyboard, terminal
//! resizes, the filesystem watcher, and later background workers for graph
//! layout, image decoding and math rendering. Giving each of those its own
//! ad-hoc polling hack inside the loop is how a TUI ends up busy-waiting and
//! redrawing sixty times a second for no reason.
//!
//! Instead every source is funnelled into one [`AppEvent`]. Blocking happens in
//! exactly one place: the terminal poll, whose timeout doubles as the tick for
//! the non-blocking sources. That keeps idle cost at one wakeup per
//! [`POLL_INTERVAL`] with no spinning.
//!
//! Deliberately no async runtime: threads and channels already cover this, and
//! pulling in Tokio to watch a directory would be a large dependency for no
//! gain.

use std::time::Duration;

use crossterm::event::{poll as poll_terminal, read as read_terminal, Event as TermEvent};
use terminalwiki_core::watch::{WikiChange, WikiWatcher};
use terminalwiki_core::{Error, Result};

/// How long the loop blocks waiting for input before checking other sources.
///
/// Short enough that a file save feels immediate, long enough that an idle TUI
/// is effectively free.
pub const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Anything the application loop can be woken up by.
#[non_exhaustive]
pub enum AppEvent {
    /// A key press, resize, mouse or paste event.
    Terminal(TermEvent),
    /// A settled batch of filesystem changes.
    ///
    /// A batch rather than a single change: a `git pull` touching hundreds of
    /// files must cause one index update and one redraw, not hundreds.
    Filesystem(Vec<WikiChange>),
    /// Nothing happened before the poll interval elapsed.
    Tick,
}

/// Multiplexes every event source into a single stream.
pub struct EventLoop {
    watcher: Option<WikiWatcher>,
}

impl EventLoop {
    /// Creates a loop driving only the terminal.
    pub fn new() -> Self {
        Self { watcher: None }
    }

    /// Attaches a filesystem watcher.
    ///
    /// Optional on purpose: if the platform refuses to watch (too many open
    /// files, an unsupported filesystem, a restricted sandbox), the TUI must
    /// still run — just without live updates.
    pub fn with_watcher(watcher: WikiWatcher) -> Self {
        Self {
            watcher: Some(watcher),
        }
    }

    /// True when live filesystem updates are active.
    pub fn is_watching(&self) -> bool {
        self.watcher.is_some()
    }

    /// Waits for the next event from any source.
    ///
    /// Terminal input has priority: a user pressing keys should never wait
    /// behind a large batch of file changes.
    pub fn next_event(&mut self) -> Result<AppEvent> {
        if poll_terminal(POLL_INTERVAL).map_err(|e| Error::other(e.to_string()))? {
            let ev = read_terminal().map_err(|e| Error::other(e.to_string()))?;
            return Ok(AppEvent::Terminal(ev));
        }

        if let Some(watcher) = self.watcher.as_mut() {
            let batch = watcher.poll_batch();
            if !batch.is_empty() {
                return Ok(AppEvent::Filesystem(batch));
            }
        }

        Ok(AppEvent::Tick)
    }
}

impl Default for EventLoop {
    fn default() -> Self {
        Self::new()
    }
}
