//! Interactive TUI for TerminalWiki (Gate 4).

pub mod app;
pub mod event;
pub mod terminal;
pub mod ui;

use std::path::PathBuf;

use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Result};

pub use app::App;
pub use terminal::TerminalGuard;

/// Launches the interactive TUI application.
pub fn run_tui(
    wikis: &WikiSet,
    config: &Config,
    initial_wiki: Option<String>,
    initial_page: Option<String>,
) -> Result<()> {
    let mut app = App::new(wikis, config, initial_wiki, initial_page)?;

    while !app.should_quit {
        // Enter RAII terminal raw mode + alternate screen
        {
            let _guard = TerminalGuard::enter()?;

            while !app.should_quit && app.should_suspend_for_editor.is_none() {
                ui::draw(&app).map_err(|e| terminalwiki_core::Error::other(e.to_string()))?;
                event::handle_event(&mut app)?;
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
