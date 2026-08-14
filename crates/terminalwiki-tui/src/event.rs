//! Input and event loop handling for TUI.

use std::time::Duration;

use crossterm::event::{
    poll as poll_event, read as read_event, Event, KeyCode, KeyEvent, KeyModifiers,
};
use terminalwiki_core::Result;

use crate::app::{App, Mode};

pub fn handle_event(app: &mut App) -> Result<()> {
    if !poll_event(Duration::from_millis(100)).map_err(|e| terminalwiki_core::Error::other(e.to_string()))? {
        return Ok(());
    }

    let event = read_event().map_err(|e| terminalwiki_core::Error::other(e.to_string()))?;

    if let Event::Key(key) = event {
        handle_key_event(app, key)?;
    }

    Ok(())
}

fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<()> {
    match app.mode {
        Mode::Normal => handle_normal_key(app, key),
        Mode::Finder => handle_finder_key(app, key),
        Mode::Backlinks => handle_backlinks_key(app, key),
        Mode::Help => handle_help_key(app, key),
        Mode::InPageSearch => handle_search_key(app, key),
    }
}

fn handle_normal_key(app: &mut App, key: KeyEvent) -> Result<()> {
    let (_, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let view_h = (rows as usize).saturating_sub(2);

    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Char('q')) | (KeyModifiers::NONE, KeyCode::Esc) => {
            app.should_quit = true;
        }
        (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
            app.scroll_down(1, view_h);
        }
        (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
            app.scroll_up(1);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
            app.scroll_down(view_h / 2, view_h);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
            app.scroll_up(view_h / 2);
        }
        (KeyModifiers::NONE, KeyCode::Char('g')) | (KeyModifiers::NONE, KeyCode::Home) => {
            app.scroll = 0;
        }
        (KeyModifiers::NONE, KeyCode::Char('G')) | (KeyModifiers::NONE, KeyCode::End) => {
            app.scroll = app.lines.len().saturating_sub(view_h);
        }
        (KeyModifiers::NONE, KeyCode::Char('f')) | (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.mode = Mode::Finder;
            app.finder_query.clear();
            app.update_finder_filter();
        }
        (KeyModifiers::NONE, KeyCode::Char('b')) => {
            app.load_backlinks();
            app.mode = Mode::Backlinks;
        }
        (KeyModifiers::NONE, KeyCode::Char('?')) => {
            app.mode = Mode::Help;
        }
        (KeyModifiers::NONE, KeyCode::Char('/')) => {
            app.mode = Mode::InPageSearch;
            app.in_page_query.clear();
        }
        (KeyModifiers::NONE, KeyCode::Char('e')) => {
            app.open_current_in_editor();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('o')) | (KeyModifiers::NONE, KeyCode::Backspace) => {
            app.go_back();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('i')) => {
            app.go_forward();
        }
        (KeyModifiers::NONE, KeyCode::Tab) => {
            if !app.extracted_links.is_empty() {
                let next = app
                    .selected_link_idx
                    .map(|i| (i + 1) % app.extracted_links.len())
                    .unwrap_or(0);
                app.selected_link_idx = Some(next);
                app.status_message = Some(format!(
                    "Selected link: [[{}]] (press Enter to open)",
                    app.extracted_links[next]
                ));
            }
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if let Some(idx) = app.selected_link_idx {
                if let Some(target) = app.extracted_links.get(idx).cloned() {
                    let _ = app.load_page(&app.current_wiki.clone(), &target, true);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_finder_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Up => {
            app.finder_selected = app.finder_selected.saturating_sub(1);
        }
        KeyCode::Down => {
            if app.finder_selected + 1 < app.finder_filtered.len() {
                app.finder_selected += 1;
            }
        }
        KeyCode::Enter => {
            if let Some((wiki, path, _)) = app.finder_filtered.get(app.finder_selected).cloned() {
                app.mode = Mode::Normal;
                let _ = app.load_page(&wiki, &path, true);
            }
        }
        KeyCode::Backspace => {
            app.finder_query.pop();
            app.update_finder_filter();
        }
        KeyCode::Char(c) => {
            app.finder_query.push(c);
            app.update_finder_filter();
        }
        _ => {}
    }
    Ok(())
}

fn handle_backlinks_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('b') => {
            app.mode = Mode::Normal;
        }
        KeyCode::Up => {
            app.backlinks_selected = app.backlinks_selected.saturating_sub(1);
        }
        KeyCode::Down => {
            if app.backlinks_selected + 1 < app.backlinks.len() {
                app.backlinks_selected += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(b) = app.backlinks.get(app.backlinks_selected).cloned() {
                app.mode = Mode::Normal;
                let path_str = b.from_relative.to_string_lossy().into_owned();
                let _ = app.load_page(&b.from_wiki, &path_str, true);
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_help_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') | KeyCode::Enter => {
            app.mode = Mode::Normal;
        }
        _ => {}
    }
    Ok(())
}

fn handle_search_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            app.mode = Mode::Normal;
            if !app.in_page_query.is_empty() {
                let q_lower = app.in_page_query.to_lowercase();
                for (i, line) in app.lines.iter().enumerate() {
                    let line_str: String = line.iter().map(|s| s.text.as_str()).collect();
                    if line_str.to_lowercase().contains(&q_lower) {
                        app.scroll = i;
                        app.status_message = Some(format!("Match found on line {}", i + 1));
                        return Ok(());
                    }
                }
                app.status_message = Some(format!("Pattern not found: {}", app.in_page_query));
            }
        }
        KeyCode::Backspace => {
            app.in_page_query.pop();
        }
        KeyCode::Char(c) => {
            app.in_page_query.push(c);
        }
        _ => {}
    }
    Ok(())
}
