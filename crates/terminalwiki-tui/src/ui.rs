//! Screen rendering and layout for TUI.

use std::io::{stdout, Write};

use crossterm::cursor::MoveTo;
use crossterm::execute;
use crossterm::style::{Color as CColor, Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{size as terminal_size, Clear, ClearType};
use terminalwiki_core::sanitize::sanitize_line;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode};

pub fn draw(app: &App) -> std::io::Result<()> {
    let (cols, rows) = terminal_size()?;
    let width = cols as usize;
    let height = rows as usize;

    if width < 10 || height < 5 {
        return Ok(());
    }

    let mut out = stdout();
    execute!(out, Clear(ClearType::All), MoveTo(0, 0))?;

    // ─── 1. Header ───────────────────────────────────────────────────────────────
    let header_bg = CColor::DarkGrey;
    let header_fg = CColor::White;
    execute!(out, SetBackgroundColor(header_bg), SetForegroundColor(header_fg))?;

    let left_text = format!(" 󰈙 {} ❯ {} ", app.current_wiki, app.current_title);
    let right_text = if !app.lines.is_empty() {
        format!(
            " Line {}/{} ({:.0}%) ",
            app.scroll + 1,
            app.lines.len(),
            ((app.scroll + 1) as f32 / app.lines.len() as f32) * 100.0
        )
    } else {
        " Empty ".to_string()
    };

    let left_w = left_text.width();
    let right_w = right_text.width();
    let pad_w = width.saturating_sub(left_w + right_w);

    execute!(out, Print(&left_text), Print(" ".repeat(pad_w)), Print(&right_text), ResetColor)?;

    // ─── 2. Content Area ─────────────────────────────────────────────────────────
    let content_height = height.saturating_sub(2);
    for row in 0..content_height {
        let line_idx = app.scroll + row;
        execute!(out, MoveTo(0, (row + 1) as u16))?;

        if let Some(line) = app.lines.get(line_idx) {
            let mut current_col = 0;
            for span in line {
                if current_col >= width {
                    break;
                }
                let span_text = sanitize_line(&span.text);
                let text_w = span_text.width();
                if current_col + text_w > width {
                    let available = width.saturating_sub(current_col);
                    let truncated: String = span_text.chars().take(available).collect();
                    execute!(out, Print(&span.style.apply(&truncated)))?;
                    current_col += available;
                } else {
                    execute!(out, Print(&span.style.apply(&span_text)))?;
                    current_col += text_w;
                }
            }
        }
    }

    // ─── 3. Overlays ─────────────────────────────────────────────────────────────
    match app.mode {
        Mode::Finder => draw_finder(app, width, height, &mut out)?,
        Mode::Backlinks => draw_backlinks(app, width, height, &mut out)?,
        Mode::Help => draw_help(width, height, &mut out)?,
        Mode::InPageSearch => draw_search_bar(app, width, height, &mut out)?,
        Mode::Normal => {}
    }

    // ─── 4. Footer Status Bar ────────────────────────────────────────────────────
    execute!(out, MoveTo(0, (height - 1) as u16), SetBackgroundColor(CColor::Black), SetForegroundColor(CColor::DarkGrey))?;
    let footer_text = match app.mode {
        Mode::Normal => {
            if let Some(ref msg) = app.status_message {
                format!("  {}", msg)
            } else {
                " [f] Find  [/] Search  [Tab] Link  [b] Backlinks  [e] Edit  [Ctrl+o/i] History  [?] Help  [q] Quit".to_string()
            }
        }
        Mode::Finder => " [↑/↓] Navigate  [Enter] Select  [Esc] Cancel".to_string(),
        Mode::Backlinks => " [↑/↓] Navigate  [Enter] Open  [Esc] Close".to_string(),
        Mode::Help => " [Esc/q/?] Close Help".to_string(),
        Mode::InPageSearch => " [Enter] Search  [Esc] Cancel".to_string(),
    };

    let foot_w = footer_text.width();
    let foot_pad = width.saturating_sub(foot_w);
    execute!(out, Print(&footer_text), Print(" ".repeat(foot_pad)), ResetColor)?;

    out.flush()?;
    Ok(())
}

fn draw_finder(app: &App, width: usize, height: usize, out: &mut std::io::Stdout) -> std::io::Result<()> {
    let dialog_w = (width * 3 / 4).clamp(40, 90).min(width);
    let dialog_h = (height * 3 / 4).clamp(10, 20).min(height);
    let start_x = (width.saturating_sub(dialog_w)) / 2;
    let start_y = (height.saturating_sub(dialog_h)) / 2;

    let bg = CColor::DarkBlue;
    let fg = CColor::White;

    for y in 0..dialog_h {
        execute!(out, MoveTo(start_x as u16, (start_y + y) as u16), SetBackgroundColor(bg), SetForegroundColor(fg))?;
        execute!(out, Print(" ".repeat(dialog_w)))?;
    }

    // Title / Prompt
    execute!(out, MoveTo((start_x + 2) as u16, (start_y + 1) as u16), SetBackgroundColor(bg), SetForegroundColor(CColor::Yellow))?;
    execute!(out, Print("🔍 Find Page: "), SetForegroundColor(CColor::White), Print(&app.finder_query), Print("█"))?;

    // Separator
    execute!(out, MoveTo((start_x + 2) as u16, (start_y + 2) as u16), SetBackgroundColor(bg), SetForegroundColor(CColor::DarkGrey))?;
    execute!(out, Print("─".repeat(dialog_w.saturating_sub(4))))?;

    // Candidate list
    let list_h = dialog_h.saturating_sub(4);
    let start_idx = app.finder_selected.saturating_sub(list_h / 2);

    for (i, (wiki, path, title)) in app.finder_filtered.iter().skip(start_idx).take(list_h).enumerate() {
        let is_selected = start_idx + i == app.finder_selected;
        let row_y = start_y + 3 + i;
        execute!(out, MoveTo((start_x + 2) as u16, row_y as u16))?;

        if is_selected {
            execute!(out, SetBackgroundColor(CColor::Cyan), SetForegroundColor(CColor::Black))?;
            let display = format!("❯ {} ({}:{})", title, wiki, path);
            let truncated: String = display.chars().take(dialog_w.saturating_sub(4)).collect();
            let pad = dialog_w.saturating_sub(4 + truncated.width());
            execute!(out, Print(&truncated), Print(" ".repeat(pad)))?;
        } else {
            execute!(out, SetBackgroundColor(bg), SetForegroundColor(CColor::White))?;
            let display = format!("  {} ", title);
            let meta = format!("({}:{})", wiki, path);
            let combined = format!("{}{}", display, meta);
            let truncated: String = combined.chars().take(dialog_w.saturating_sub(4)).collect();
            let pad = dialog_w.saturating_sub(4 + truncated.width());
            execute!(out, Print(&truncated), Print(" ".repeat(pad)))?;
        }
    }

    execute!(out, ResetColor)?;
    Ok(())
}

fn draw_backlinks(app: &App, width: usize, height: usize, out: &mut std::io::Stdout) -> std::io::Result<()> {
    let dialog_w = (width * 3 / 4).clamp(40, 80).min(width);
    let dialog_h = (height * 3 / 4).clamp(10, 18).min(height);
    let start_x = (width.saturating_sub(dialog_w)) / 2;
    let start_y = (height.saturating_sub(dialog_h)) / 2;

    let bg = CColor::DarkMagenta;
    let fg = CColor::White;

    for y in 0..dialog_h {
        execute!(out, MoveTo(start_x as u16, (start_y + y) as u16), SetBackgroundColor(bg), SetForegroundColor(fg))?;
        execute!(out, Print(" ".repeat(dialog_w)))?;
    }

    execute!(out, MoveTo((start_x + 2) as u16, (start_y + 1) as u16), SetBackgroundColor(bg), SetForegroundColor(CColor::Yellow))?;
    execute!(out, Print(format!("🔗 Backlinks to '{}' ({}):", app.current_title, app.backlinks.len())))?;

    let list_h = dialog_h.saturating_sub(3);
    for (i, b) in app.backlinks.iter().take(list_h).enumerate() {
        let is_selected = i == app.backlinks_selected;
        let row_y = start_y + 3 + i;
        execute!(out, MoveTo((start_x + 2) as u16, row_y as u16))?;

        if is_selected {
            execute!(out, SetBackgroundColor(CColor::White), SetForegroundColor(CColor::Black))?;
            let display = format!("❯ {} ({}:{})", b.from_title, b.from_wiki, b.from_relative.display());
            let truncated: String = display.chars().take(dialog_w.saturating_sub(4)).collect();
            execute!(out, Print(&truncated))?;
        } else {
            execute!(out, SetBackgroundColor(bg), SetForegroundColor(CColor::White))?;
            let display = format!("  {} ({}:{})", b.from_title, b.from_wiki, b.from_relative.display());
            let truncated: String = display.chars().take(dialog_w.saturating_sub(4)).collect();
            execute!(out, Print(&truncated))?;
        }
    }

    execute!(out, ResetColor)?;
    Ok(())
}

fn draw_help(width: usize, height: usize, out: &mut std::io::Stdout) -> std::io::Result<()> {
    let dialog_w = 60.min(width);
    let dialog_h = 16.min(height);
    let start_x = (width.saturating_sub(dialog_w)) / 2;
    let start_y = (height.saturating_sub(dialog_h)) / 2;

    let bg = CColor::DarkGrey;
    let fg = CColor::White;

    for y in 0..dialog_h {
        execute!(out, MoveTo(start_x as u16, (start_y + y) as u16), SetBackgroundColor(bg), SetForegroundColor(fg))?;
        execute!(out, Print(" ".repeat(dialog_w)))?;
    }

    let help_lines = [
        "  TerminalWiki TUI Keybindings",
        "  ──────────────────────────────────────────",
        "  j / ↓         Scroll down",
        "  k / ↑         Scroll up",
        "  Ctrl+d / u    Half-page down / up",
        "  g / G         Jump to top / bottom",
        "  Tab / S-Tab   Cycle through page links",
        "  Enter         Follow selected link",
        "  f / Ctrl+p    Open fuzzy finder",
        "  b             Toggle backlinks pane",
        "  e             Edit current page in $EDITOR",
        "  Ctrl+o        Go back in navigation history",
        "  Ctrl+i        Go forward in history",
        "  ?             Toggle this help modal",
        "  q / Esc       Quit / Close modal",
    ];

    for (i, line) in help_lines.iter().enumerate() {
        if 1 + i < dialog_h {
            execute!(out, MoveTo((start_x + 2) as u16, (start_y + 1 + i) as u16), SetBackgroundColor(bg), SetForegroundColor(CColor::White))?;
            execute!(out, Print(line))?;
        }
    }

    execute!(out, ResetColor)?;
    Ok(())
}

fn draw_search_bar(app: &App, width: usize, height: usize, out: &mut std::io::Stdout) -> std::io::Result<()> {
    execute!(out, MoveTo(0, (height - 2) as u16), SetBackgroundColor(CColor::Blue), SetForegroundColor(CColor::White))?;
    let prompt = format!(" /{}█", app.in_page_query);
    let pad = width.saturating_sub(prompt.width());
    execute!(out, Print(&prompt), Print(" ".repeat(pad)), ResetColor)?;
    Ok(())
}
