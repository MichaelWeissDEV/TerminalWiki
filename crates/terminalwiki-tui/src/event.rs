//! Input and key event handling for TUI (spec §40-§53).

use std::time::Duration;

use crossterm::event::{
    poll as poll_event, read as read_event, Event, KeyCode, KeyEvent, KeyModifiers,
};
use terminalwiki_core::Result;
use terminalwiki_render::LinkTarget;

use crate::app::{App, Mode, PageTemplate, SidebarMode};

/// Dispatches one terminal event delivered by the event runtime.
pub fn handle_terminal_event(app: &mut App, event: Event) -> Result<()> {
    match event {
        Event::Key(key) => handle_key_event(app, key),
        // A resize only needs a redraw, which the caller performs.
        Event::Resize(_, _) => Ok(()),
        _ => Ok(()),
    }
}

/// Polls the terminal directly. Retained for callers outside the event runtime.
pub fn handle_event(app: &mut App) -> Result<()> {
    if !poll_event(Duration::from_millis(100))
        .map_err(|e| terminalwiki_core::Error::other(e.to_string()))?
    {
        return Ok(());
    }

    let event = read_event().map_err(|e| terminalwiki_core::Error::other(e.to_string()))?;
    handle_terminal_event(app, event)
}

fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<()> {
    match app.mode {
        Mode::Normal => handle_normal_key(app, key),
        Mode::Finder => handle_finder_key(app, key),
        Mode::Outline => handle_outline_key(app, key),
        Mode::Backlinks => handle_backlinks_key(app, key),
        Mode::Command => handle_command_key(app, key),
        Mode::Help => handle_help_key(app, key),
        Mode::InPageSearch => handle_search_key(app, key),
        Mode::WikiSearch => handle_wiki_search_key(app, key),
        Mode::Graph => handle_graph_key(app, key),
        Mode::Sidebar => handle_sidebar_key(app, key),
        Mode::NewPage => handle_new_page_key(app, key),
        Mode::TagPrompt => handle_tag_prompt_key(app, key),
    }
}

fn handle_normal_key(app: &mut App, key: KeyEvent) -> Result<()> {
    let (_, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let view_h = (rows as usize).saturating_sub(3);

    // While the "page removed from disk" notice is shown, the only meaningful
    // actions are going back or retrying; everything else would operate on a
    // page that no longer exists.
    if app.page_missing {
        match (key.modifiers, key.code) {
            (KeyModifiers::NONE, KeyCode::Char('b')) | (KeyModifiers::NONE, KeyCode::Backspace) => {
                app.page_missing = false;
                app.go_back();
                return Ok(());
            }
            (KeyModifiers::NONE, KeyCode::Char('r')) => {
                app.reload_current_page();
                return Ok(());
            }
            (KeyModifiers::NONE, KeyCode::Char('q')) | (KeyModifiers::NONE, KeyCode::Esc) => {
                app.should_quit = true;
                return Ok(());
            }
            _ => {}
        }
    }

    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Char('q')) => {
            app.should_quit = true;
        }
        // Esc never leaves the application from the reader: it only clears the
        // transient status line, so a stray keypress cannot drop the user back
        // into the shell.
        (KeyModifiers::NONE, KeyCode::Esc) => {
            app.status_message = None;
        }
        (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
            app.scroll_down(1, view_h);
        }
        (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
            app.scroll_up(1);
        }
        (KeyModifiers::NONE, KeyCode::Char('h')) | (KeyModifiers::NONE, KeyCode::Left) => {
            app.scroll_left(4);
        }
        (KeyModifiers::NONE, KeyCode::Char('l')) | (KeyModifiers::NONE, KeyCode::Right) => {
            app.scroll_right(4);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
            app.scroll_down(view_h / 2, view_h);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
            app.scroll_up(view_h / 2);
        }
        // `g` opens the graph (spec item 101). Scroll-to-top remains on Home;
        // the older `g`-as-top binding is superseded.
        (KeyModifiers::NONE, KeyCode::Home) => {
            app.scroll = 0;
        }
        (KeyModifiers::NONE, KeyCode::Char('g')) => {
            app.open_graph();
        }
        (KeyModifiers::NONE, KeyCode::Char('G')) | (KeyModifiers::NONE, KeyCode::End) => {
            app.scroll = app.lines.len().saturating_sub(view_h);
        }
        (KeyModifiers::NONE, KeyCode::Char('f')) | (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.mode = Mode::Finder;
            app.finder_query.clear();
            app.update_finder_filter();
        }
        (KeyModifiers::NONE, KeyCode::Char('>'))
        | (KeyModifiers::SHIFT, KeyCode::Char('>'))
        | (KeyModifiers::CONTROL | KeyModifiers::SHIFT, KeyCode::Char('P')) => {
            app.mode = Mode::Finder;
            app.finder_query = ">".to_string();
            app.update_finder_filter();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('w')) => {
            app.mode = Mode::WikiSearch;
            app.wiki_search_query.clear();
            app.update_wiki_search();
        }
        (KeyModifiers::NONE, KeyCode::Char('o')) => {
            if !app.headings.is_empty() {
                app.mode = Mode::Outline;
                app.outline_selected = 0;
            } else {
                app.status_message = Some("No headings in current document".to_string());
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('b')) => {
            app.load_backlinks();
            app.mode = Mode::Backlinks;
        }
        (KeyModifiers::NONE, KeyCode::Char(':')) => {
            app.mode = Mode::Command;
            app.command_input.clear();
            app.update_command_suggestions();
        }
        (KeyModifiers::NONE, KeyCode::Char('?')) => {
            app.mode = Mode::Help;
            app.scroll = 0;
        }
        (KeyModifiers::NONE, KeyCode::Char('/')) | (KeyModifiers::CONTROL, KeyCode::Char('f')) => {
            app.mode = Mode::InPageSearch;
            app.in_page_query.clear();
            app.search_matches.clear();
            app.search_match_idx = 0;
        }
        (KeyModifiers::NONE, KeyCode::F(3)) | (KeyModifiers::CONTROL, KeyCode::Char('g')) => {
            app.next_search_match();
        }
        (KeyModifiers::NONE, KeyCode::Char('e')) => {
            app.open_current_in_editor();
        }
        (KeyModifiers::NONE, KeyCode::Char('s')) => {
            app.toggle_sidebar();
        }
        (KeyModifiers::NONE, KeyCode::Char('v')) => {
            app.toggle_sidebar_mode();
        }
        (KeyModifiers::NONE, KeyCode::Char('z')) => {
            app.toggle_zen_mode();
        }
        (KeyModifiers::NONE, KeyCode::Char('w')) => {
            app.toggle_wikipedia_layout();
        }
        (KeyModifiers::NONE, KeyCode::Char('t')) => {
            app.mode = Mode::TagPrompt;
            app.tag_input.clear();
        }
        (KeyModifiers::NONE, KeyCode::Char('n')) => {
            app.begin_new_page(PageTemplate::Blank);
        }
        (KeyModifiers::NONE, KeyCode::Char('T')) | (KeyModifiers::SHIFT, KeyCode::Char('T')) => {
            app.begin_new_page(PageTemplate::Todo);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('n'))
        | (KeyModifiers::NONE, KeyCode::Char('Q'))
        | (KeyModifiers::SHIFT, KeyCode::Char('Q')) => {
            app.begin_new_page(PageTemplate::QuickNote);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('o')) | (KeyModifiers::NONE, KeyCode::Backspace) => {
            app.go_back();
        }
        (KeyModifiers::CONTROL, KeyCode::Char('i')) => {
            app.go_forward();
        }
        (KeyModifiers::NONE, KeyCode::Char('d')) => {
            let _ = app.create_daily_note();
        }
        (KeyModifiers::NONE, KeyCode::Char('m')) => {
            app.toggle_bookmark();
        }
        (KeyModifiers::SHIFT, KeyCode::Char('L')) | (KeyModifiers::NONE, KeyCode::Char('L')) => {
            app.toggle_link_filter();
        }
        (KeyModifiers::NONE, KeyCode::Char('x')) | (KeyModifiers::NONE, KeyCode::Char(' ')) => {
            let _ = app.toggle_todo_on_current_line();
        }
        (KeyModifiers::NONE, KeyCode::Tab) | (KeyModifiers::NONE, KeyCode::Char(']')) => {
            app.cycle_links(true, view_h);
        }
        (KeyModifiers::SHIFT, KeyCode::BackTab)
        | (KeyModifiers::NONE, KeyCode::BackTab)
        | (KeyModifiers::NONE, KeyCode::Char('[')) => {
            app.cycle_links(false, view_h);
        }
        (KeyModifiers::NONE, KeyCode::Char(digit @ '1'..='9')) => {
            let link_idx = (digit as usize) - ('1' as usize);
            if link_idx < app.links.len() {
                app.selected_link_idx = Some(link_idx);
                follow_selected_link(app);
            }
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            follow_selected_link(app);
        }
        _ => {}
    }
    Ok(())
}

fn open_url_in_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
    }
    Ok(())
}

fn follow_selected_link(app: &mut App) {
    if let Some(idx) = app.selected_link_idx {
        if let Some(target) = app.links.get(idx).map(|l| l.target.clone()) {
            match target {
                LinkTarget::Wiki { wiki, page, anchor } => {
                    let target_wiki = wiki.unwrap_or_else(|| app.current_wiki.clone());
                    if app.load_page(&target_wiki, &page, true).is_ok() {
                        if let Some(anchor) = anchor {
                            jump_to_anchor(app, &anchor);
                        }
                    }
                }
                LinkTarget::External(url) => match open_url_in_browser(&url) {
                    Ok(()) => {
                        app.status_message = Some(format!("Opened in browser: {url}"));
                    }
                    Err(err) => {
                        app.status_message = Some(format!("Could not open browser ({err}): {url}"));
                    }
                },
                LinkTarget::File { path, line_range } => {
                    let wiki = app.current_wiki.clone();
                    let path_str = path.to_string_lossy().into_owned();
                    if app.load_page(&wiki, &path_str, true).is_ok() {
                        if let Some((start, _)) = line_range {
                            app.scroll = start.saturating_sub(1);
                        }
                    }
                }
                LinkTarget::Heading(anchor) => {
                    jump_to_anchor(app, &anchor);
                }
                LinkTarget::Tag(tag) => {
                    app.mode = Mode::Finder;
                    app.finder_query = format!("#{tag}");
                    app.update_finder_filter();
                    app.status_message = Some(format!(
                        "Tag #{tag}: {} matching pages",
                        app.finder_filtered.len()
                    ));
                }
            }
        }
    }
}

/// Resolves link anchors with the same GitHub-compatible normalization used by
/// core link parsing, so `[[Page#Heading Name]]` and `[[#heading-name]]` land
/// on the same heading regardless of title casing or punctuation.
fn jump_to_anchor(app: &mut App, anchor: &str) {
    let wanted = terminalwiki_core::link::heading_anchor(anchor);
    if let Some(heading) = app.headings.iter().find(|heading| heading.anchor == wanted) {
        app.scroll = heading.line;
        app.status_message = Some(format!("Jumped to {}", heading.text));
    } else {
        app.status_message = Some(format!("Heading not found: {anchor}"));
    }
}

fn handle_finder_key(app: &mut App, key: KeyEvent) -> Result<()> {
    let item_count = if app.finder_is_command_mode() {
        app.finder_commands.len()
    } else {
        app.finder_filtered.len()
    };

    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) => {
            app.mode = Mode::Normal;
        }
        (_, KeyCode::Up)
        | (KeyModifiers::CONTROL, KeyCode::Char('k'))
        | (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.finder_selected = app.finder_selected.saturating_sub(1);
        }
        (_, KeyCode::Down)
        | (KeyModifiers::CONTROL, KeyCode::Char('j'))
        | (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
            if app.finder_selected + 1 < item_count {
                app.finder_selected += 1;
            }
        }
        (_, KeyCode::Enter) => {
            app.execute_finder_selection();
        }
        (_, KeyCode::Backspace) => {
            app.finder_query.pop();
            app.update_finder_filter();
        }
        (_, KeyCode::Char(c)) => {
            app.finder_query.push(c);
            app.update_finder_filter();
        }
        _ => {}
    }
    Ok(())
}

fn handle_outline_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('o') => {
            app.mode = Mode::Normal;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.outline_selected = app.outline_selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.outline_selected + 1 < app.headings.len() {
                app.outline_selected += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(heading) = app.headings.get(app.outline_selected) {
                app.scroll = heading.line;
                app.mode = Mode::Normal;
            }
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
        KeyCode::Up | KeyCode::Char('k') => {
            app.backlinks_selected = app.backlinks_selected.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
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

fn handle_command_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
        }
        KeyCode::Enter => {
            app.mode = Mode::Normal;
            app.execute_command();
        }
        KeyCode::Tab => {
            if let Some(first) = app.command_suggestions.first() {
                app.command_input = first.clone();
            }
        }
        KeyCode::Backspace => {
            app.command_input.pop();
            app.update_command_suggestions();
        }
        KeyCode::Char(c) => {
            app.command_input.push(c);
            app.update_command_suggestions();
        }
        _ => {}
    }
    Ok(())
}

fn handle_help_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc)
        | (KeyModifiers::NONE, KeyCode::Char('q'))
        | (KeyModifiers::NONE, KeyCode::Char('?'))
        | (KeyModifiers::NONE, KeyCode::Enter) => {
            app.mode = Mode::Normal;
            app.scroll = 0;
        }
        (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
            app.scroll = app.scroll.saturating_add(1);
        }
        (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
            app.scroll = app.scroll.saturating_sub(1);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('d')) | (KeyModifiers::NONE, KeyCode::PageDown) => {
            app.scroll = app.scroll.saturating_add(10);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) | (KeyModifiers::NONE, KeyCode::PageUp) => {
            app.scroll = app.scroll.saturating_sub(10);
        }
        (KeyModifiers::NONE, KeyCode::Home) => {
            app.scroll = 0;
        }
        _ => {}
    }
    Ok(())
}

fn handle_search_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) => {
            app.mode = Mode::Normal;
        }
        (_, KeyCode::Down)
        | (KeyModifiers::CONTROL, KeyCode::Char('n'))
        | (KeyModifiers::NONE, KeyCode::Tab) => {
            app.next_search_match();
        }
        (_, KeyCode::Up)
        | (KeyModifiers::CONTROL, KeyCode::Char('p'))
        | (KeyModifiers::SHIFT, KeyCode::BackTab) => {
            app.prev_search_match();
        }
        (_, KeyCode::Enter) => {
            app.mode = Mode::Normal;
            if !app.in_page_query.is_empty() {
                app.update_in_page_search();
                if let Some(&line) = app.search_matches.get(app.search_match_idx) {
                    app.scroll = line;
                    app.status_message = Some(format!(
                        "Match {}/{} on line {} (F3 / Ctrl+G for next)",
                        app.search_match_idx + 1,
                        app.search_matches.len(),
                        line + 1
                    ));
                } else {
                    app.status_message = Some(format!("Pattern not found: {}", app.in_page_query));
                }
            }
        }
        (_, KeyCode::Backspace) => {
            app.in_page_query.pop();
            app.update_in_page_search();
        }
        (_, KeyCode::Char(c)) => {
            app.in_page_query.push(c);
            app.update_in_page_search();
        }
        _ => {}
    }
    Ok(())
}

fn handle_wiki_search_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) => {
            app.mode = Mode::Normal;
        }
        (_, KeyCode::Up)
        | (KeyModifiers::CONTROL, KeyCode::Char('k'))
        | (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.wiki_search_selected = app.wiki_search_selected.saturating_sub(1);
        }
        (_, KeyCode::Down)
        | (KeyModifiers::CONTROL, KeyCode::Char('j'))
        | (KeyModifiers::CONTROL, KeyCode::Char('n')) => {
            if app.wiki_search_selected + 1 < app.wiki_search_hits.len() {
                app.wiki_search_selected += 1;
            }
        }
        (_, KeyCode::Enter) => {
            app.open_wiki_search_selected();
        }
        (_, KeyCode::Backspace) => {
            app.wiki_search_query.pop();
            app.update_wiki_search();
        }
        (_, KeyCode::Char(c)) => {
            app.wiki_search_query.push(c);
            app.update_wiki_search();
        }
        _ => {}
    }
    Ok(())
}

/// Sidebar explorer keys. `Esc` returns to the article while keeping the
/// sidebar visible; `s` hides it entirely.
fn handle_sidebar_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) => {
            app.mode = Mode::Normal;
        }
        (KeyModifiers::NONE, KeyCode::Char('s')) => {
            app.sidebar_visible = false;
            app.mode = Mode::Normal;
        }
        (KeyModifiers::NONE, KeyCode::Char('v')) => {
            app.set_sidebar_mode(SidebarMode::Tree);
        }
        (KeyModifiers::NONE, KeyCode::Char('f')) => {
            app.set_sidebar_mode(SidebarMode::Flat);
        }
        (KeyModifiers::NONE, KeyCode::Char('o')) => {
            app.set_sidebar_mode(SidebarMode::Outline);
        }
        (KeyModifiers::NONE, KeyCode::Char('t')) => {
            app.set_sidebar_mode(SidebarMode::Tags);
        }
        (KeyModifiers::NONE, KeyCode::Char('b')) | (KeyModifiers::NONE, KeyCode::Char('m')) => {
            app.set_sidebar_mode(SidebarMode::Bookmarks);
        }
        (KeyModifiers::NONE, KeyCode::Tab) => {
            app.toggle_sidebar_mode();
        }
        (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
            app.sidebar_select(1);
        }
        (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
            app.sidebar_select(-1);
        }
        (KeyModifiers::NONE, KeyCode::Enter) | (KeyModifiers::NONE, KeyCode::Char('l')) => {
            if let Err(error) = app.open_sidebar_selected() {
                app.status_message = Some(error.to_string());
            }
        }
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => {
            app.mode = Mode::Finder;
            app.finder_query.clear();
            app.update_finder_filter();
        }
        (KeyModifiers::NONE, KeyCode::Char('n')) => {
            app.begin_new_page(PageTemplate::Blank);
        }
        (KeyModifiers::NONE, KeyCode::Char('T')) | (KeyModifiers::SHIFT, KeyCode::Char('T')) => {
            app.begin_new_page(PageTemplate::Todo);
        }
        (KeyModifiers::CONTROL, KeyCode::Char('n'))
        | (KeyModifiers::NONE, KeyCode::Char('Q'))
        | (KeyModifiers::SHIFT, KeyCode::Char('Q')) => {
            app.begin_new_page(PageTemplate::QuickNote);
        }
        (KeyModifiers::NONE, KeyCode::Char('?')) => {
            app.mode = Mode::Help;
            app.scroll = 0;
        }
        (KeyModifiers::NONE, KeyCode::Char('q')) => {
            app.should_quit = true;
        }
        _ => {}
    }
    Ok(())
}

/// Page-creation prompt keys. Tab switches the starter template, Enter writes
/// the file and opens it, Ctrl+E writes and immediately opens $EDITOR, Esc cancels.
fn handle_new_page_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) => {
            app.mode = Mode::Normal;
            app.status_message = Some("New page cancelled".to_string());
        }
        (KeyModifiers::CONTROL, KeyCode::Char('e')) => match app.create_page_from_input() {
            Ok(()) => app.open_current_in_editor(),
            Err(error) => app.status_message = Some(error.to_string()),
        },
        (_, KeyCode::Enter) => {
            if let Err(error) = app.create_page_from_input() {
                app.status_message = Some(error.to_string());
            }
        }
        (_, KeyCode::Tab) => {
            app.toggle_new_page_template();
        }
        (_, KeyCode::Backspace) => {
            app.new_page_name.pop();
        }
        (_, KeyCode::Char(character)) => {
            app.new_page_name.push(character);
        }
        _ => {}
    }
    Ok(())
}

fn handle_tag_prompt_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.status_message = Some("Tagging cancelled".to_string());
        }
        KeyCode::Enter => {
            let tag = app.tag_input.clone();
            match app.add_tag_to_current_page(&tag) {
                Ok(()) => {
                    app.mode = Mode::Normal;
                }
                Err(err) => {
                    app.status_message = Some(err.to_string());
                }
            }
        }
        KeyCode::Backspace => {
            app.tag_input.pop();
        }
        KeyCode::Char(c) => {
            app.tag_input.push(c);
        }
        _ => {}
    }
    Ok(())
}

/// Graph view keys (spec items 44-45, old Gate 7.7).
fn handle_graph_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match (key.modifiers, key.code) {
        (_, KeyCode::Esc) | (KeyModifiers::NONE, KeyCode::Char('q')) => {
            app.close_graph();
        }
        (KeyModifiers::NONE, KeyCode::Char('j')) | (KeyModifiers::NONE, KeyCode::Down) => {
            app.graph_select(1);
        }
        (KeyModifiers::NONE, KeyCode::Char('k')) | (KeyModifiers::NONE, KeyCode::Up) => {
            app.graph_select(-1);
        }
        // Tab cycles in the same spatial order as j/k.
        (KeyModifiers::NONE, KeyCode::Tab) => {
            app.graph_select(1);
        }
        (KeyModifiers::SHIFT, KeyCode::BackTab) | (KeyModifiers::NONE, KeyCode::BackTab) => {
            app.graph_select(-1);
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            app.graph_open_selected();
        }
        (KeyModifiers::NONE, KeyCode::Char('a')) | (KeyModifiers::NONE, KeyCode::Char('g')) => {
            app.toggle_graph_scope();
        }
        (KeyModifiers::NONE, KeyCode::Char('+')) | (KeyModifiers::NONE, KeyCode::Char('=')) => {
            app.graph_change_depth(1);
        }
        (KeyModifiers::NONE, KeyCode::Char('-')) => {
            app.graph_change_depth(-1);
        }
        (KeyModifiers::NONE, KeyCode::Char('r')) => {
            let depth = app.graph_view.as_ref().map(|v| v.depth);
            if let Some(d) = depth {
                app.invalidate_graph();
                app.open_graph_at_depth(d);
                app.status_message = Some("Graph reloaded".to_string());
            }
        }
        (KeyModifiers::NONE, KeyCode::Char('?')) => {
            app.mode = Mode::Help;
            app.scroll = 0;
        }
        _ => {}
    }
    Ok(())
}
