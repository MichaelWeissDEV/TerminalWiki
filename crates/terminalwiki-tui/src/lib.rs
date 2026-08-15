//! Interactive TUI for TerminalWiki (Gate 4).

pub mod app;
pub mod event;
pub mod runtime;
pub mod terminal;
pub mod ui;

use std::path::PathBuf;

use terminalwiki_core::watch::WikiWatcher;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Result};

pub use app::App;
pub use runtime::{AppEvent, EventLoop};
pub use terminal::TerminalGuard;

/// Launches the interactive TUI application.
pub fn run_tui(
    wikis: &WikiSet,
    config: &Config,
    initial_wiki: Option<String>,
    initial_page: Option<String>,
) -> Result<()> {
    // Reconcile before building the index-backed view, so a file created since
    // the last run is already present when the finder opens (spec Gate 1.3).
    // Done once at startup; from here the watcher drives updates.
    if config.index.auto_update {
        for wiki in wikis.iter() {
            let _ = terminalwiki_index::WikiIndex::open(wiki, config);
        }
    }

    let mut app = App::new(wikis, config, initial_wiki, initial_page)?;

    while !app.should_quit {
        // Enter RAII terminal raw mode + alternate screen
        {
            let _guard = TerminalGuard::enter()?;

            // The watcher is recreated per session so it picks up wikis added
            // while the editor was suspended. Failure is not fatal: the TUI
            // simply runs without live updates.
            let mut events = match build_watcher(wikis) {
                Some(watcher) => EventLoop::with_watcher(watcher),
                None => {
                    app.status_message =
                        Some("Live file updates unavailable; use :reload".to_string());
                    EventLoop::new()
                }
            };

            ui::draw(&app).map_err(|e| terminalwiki_core::Error::other(e.to_string()))?;

            while !app.should_quit && app.should_suspend_for_editor.is_none() {
                // Redraw only in response to something, so an idle TUI costs
                // one wakeup per poll interval rather than a redraw per frame.
                let redraw = match events.next_event()? {
                    AppEvent::Terminal(ev) => {
                        event::handle_terminal_event(&mut app, ev)?;
                        true
                    }
                    AppEvent::Filesystem(changes) => app.apply_fs_changes(&changes),
                    AppEvent::Tick => false,
                };

                if redraw {
                    ui::draw(&app).map_err(|e| terminalwiki_core::Error::other(e.to_string()))?;
                }
            }
        } // TerminalGuard dropped here: terminal restored to normal mode

        // If suspended to open editor
        if let Some(target_path) = app.should_suspend_for_editor.take() {
            open_editor_subprocess(&target_path, config)?;
            // Reload page upon return
            let page_str = app.current_path.to_string_lossy().into_owned();
            let _ = app.load_page(&app.current_wiki.clone(), &page_str, false);
        }
    }

    Ok(())
}

/// Builds a watcher covering every registered wiki and its mounts.
///
/// Returns `None` if no wiki could be watched, which keeps the TUI usable on
/// platforms or sandboxes where watching is unavailable.
fn build_watcher(wikis: &WikiSet) -> Option<WikiWatcher> {
    // Every registered wiki is watched. Subwikis are mounted *by name* and are
    // themselves registered wikis, so iterating the set already covers them.
    let roots: Vec<(String, PathBuf)> = wikis
        .iter()
        .map(|w| (w.name.clone(), w.root.clone()))
        .collect();
    if roots.is_empty() {
        return None;
    }
    WikiWatcher::new(roots).ok()
}

fn open_editor_subprocess(path: &PathBuf, config: &Config) -> Result<()> {
    let editor = config
        .editor
        .as_ref()
        .filter(|s| !s.is_empty())
        .cloned()
        .or_else(|| std::env::var("TW_EDITOR").ok().filter(|s| !s.is_empty()))
        .or_else(|| std::env::var("VISUAL").ok().filter(|s| !s.is_empty()))
        .or_else(|| std::env::var("EDITOR").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "nvim".to_string());

    let parts: Vec<&str> = editor.split_whitespace().collect();
    if parts.is_empty() {
        return Err(terminalwiki_core::Error::other("Editor is empty"));
    }

    let mut cmd = std::process::Command::new(parts[0]);
    for arg in &parts[1..] {
        cmd.arg(arg);
    }
    cmd.arg(path);

    let status = cmd
        .status()
        .map_err(|e| terminalwiki_core::Error::other(format!("Failed to spawn editor: {e}")))?;

    if !status.success() {
        return Err(terminalwiki_core::Error::other(format!(
            "Editor exited with code: {status}"
        )));
    }

    Ok(())
}
