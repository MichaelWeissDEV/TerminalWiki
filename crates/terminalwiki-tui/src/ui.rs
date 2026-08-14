//! Screen rendering and minimalist terminal-native layout for TUI (spec §45-§55).

use std::io::{stdout, Write};

use crossterm::cursor::MoveTo;
use crossterm::execute;
use crossterm::style::{Color as CColor, Print, ResetColor, SetAttribute, SetForegroundColor, Attribute};
use crossterm::terminal::{size as terminal_size, Clear, ClearType};
use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::unicode::{display_width, pad_display_width, truncate_display_width};

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

    // ─── 1. Minimal Header (Phase 46) ─────────────────────────────────────────────
    let path_str = app.current_path.to_string_lossy();
    let title_display = if path_str.is_empty() {
        format!("{} · Home", app.current_wiki)
    } else {
        format!("{} / {}", app.current_wiki, path_str)
    };

    let percent_str = if !app.lines.is_empty() {
        format!(
            "{:.0}%",
            ((app.scroll + 1) as f32 / app.lines.len() as f32) * 100.0
        )
    } else {
        "100%".to_string()
    };

    let title_w = display_width(&title_display);
    let pct_w = display_width(&percent_str);
    let pad_len = width.saturating_sub(title_w + pct_w + 4);

    execute!(
        out,
        SetForegroundColor(CColor::White),
        SetAttribute(Attribute::Bold),
        Print(format!("  {title_display}")),
        SetAttribute(Attribute::Reset),
        Print(" ".repeat(pad_len)),
        SetForegroundColor(CColor::DarkGrey),
        Print(format!("{percent_str}  ")),
        ResetColor
    )?;

    // Thin separator
    execute!(
        out,
        MoveTo(0, 1),
        SetForegroundColor(CColor::DarkGrey),
        Print("─".repeat(width)),
        ResetColor
    )?;

    // ─── 2. Content Viewport ─────────────────────────────────────────────────────
    let content_height = height.saturating_sub(3);
    for row in 0..content_height {
        let line_idx = app.scroll + row;
        execute!(out, MoveTo(0, (row + 2) as u16))?;

        if let Some(line) = app.lines.get(line_idx) {
            let mut current_col = 0;
            for span in line {
                if current_col >= width {
                    break;
                }
                let span_text = sanitize_line(&span.text);
                let text_w = display_width(&span_text);
                if current_col + text_w > width {
                    let available = width.saturating_sub(current_col);
                    let truncated = truncate_display_width(&span_text, available);
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
        Mode::Finder => draw_minimal_finder(app, width, height, &mut out)?,
        Mode::Outline => draw_outline(app, width, height, &mut out)?,
        Mode::Backlinks => draw_backlinks(app, width, height, &mut out)?,
        Mode::Help => draw_help(width, height, &mut out)?,
        Mode::InPageSearch => draw_search_bar(app, width, height, &mut out)?,
        Mode::Normal => {}
    }

    // ─── 4. Footer Status Line (Phase 47) ─────────────────────────────────────────
    execute!(out, MoveTo(0, (height - 1) as u16))?;
    let (left_text, right_text) = match app.mode {
        Mode::Normal => {
            if let Some(ref msg) = app.status_message {
                (format!("  {msg}"), format!("{}  ", app.current_wiki))
            } else {
                (format!("  {}", app.current_path.display()), format!("{}  ", app.current_wiki))
            }
        }
        Mode::Finder => ("  [Up/Down] Select  [Enter] Open  [Esc] Cancel".to_string(), "Finder  ".to_string()),
        Mode::Outline => ("  [Up/Down] Section  [Enter] Jump  [Esc] Close".to_string(), "Outline  ".to_string()),
        Mode::Backlinks => ("  [Up/Down] Backlink  [Enter] Open  [Esc] Close".to_string(), "Backlinks  ".to_string()),
        Mode::Help => ("  [Esc/q/?] Close Help".to_string(), "Help  ".to_string()),
        Mode::InPageSearch => ("  [Enter] Search  [Esc] Cancel".to_string(), "Search  ".to_string()),
    };

    let left_w = display_width(&left_text);
    let right_w = display_width(&right_text);
    let foot_pad = width.saturating_sub(left_w + right_w);

    execute!(
        out,
        SetForegroundColor(CColor::DarkGrey),
        Print(&left_text),
        Print(" ".repeat(foot_pad)),
        Print(&right_text),
        ResetColor
    )?;

    out.flush()?;
    Ok(())
}

fn draw_minimal_finder(
    app: &App,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    let dialog_w = (width * 4 / 5).clamp(40, 80).min(width);
    let dialog_h = (height * 3 / 4).clamp(8, 16).min(height);
    let start_x = (width.saturating_sub(dialog_w)) / 2;
    let start_y = (height.saturating_sub(dialog_h)) / 2;

    for y in 0..dialog_h {
        execute!(out, MoveTo(start_x as u16, (start_y + y) as u16), SetForegroundColor(CColor::White))?;
        execute!(out, Print(" ".repeat(dialog_w)))?;
    }

    // Prompt line
    execute!(out, MoveTo((start_x + 2) as u16, (start_y + 1) as u16), SetForegroundColor(CColor::Cyan))?;
    execute!(out, Print("> "), SetForegroundColor(CColor::White), Print(&app.finder_query), Print("_"))?;

    // Separator rule
    execute!(out, MoveTo((start_x + 2) as u16, (start_y + 2) as u16), SetForegroundColor(CColor::DarkGrey))?;
    execute!(out, Print("─".repeat(dialog_w.saturating_sub(4))))?;

    // Candidate list
    let list_h = dialog_h.saturating_sub(4);
    let start_idx = app.finder_selected.saturating_sub(list_h / 2);

    for (i, hit) in app.finder_filtered.iter().skip(start_idx).take(list_h).enumerate() {
        let is_selected = start_idx + i == app.finder_selected;
        let row_y = start_y + 3 + i;
        execute!(out, MoveTo((start_x + 2) as u16, row_y as u16))?;

        let title_str = if hit.title.is_empty() {
            hit.relative.to_string_lossy().to_string()
        } else {
            hit.title.clone()
        };
        let meta_str = format!("{}:{}", hit.wiki, hit.relative.display());

        if is_selected {
            execute!(out, SetForegroundColor(CColor::Cyan), SetAttribute(Attribute::Bold))?;
            let display = format!("> {}", title_str);
            let padded = pad_display_width(&display, dialog_w.saturating_sub(meta_str.len() + 6));
            execute!(out, Print(&padded), SetAttribute(Attribute::Reset), SetForegroundColor(CColor::DarkGrey), Print(&meta_str))?;
        } else {
            execute!(out, SetForegroundColor(CColor::White))?;
            let display = format!("  {}", title_str);
            let padded = pad_display_width(&display, dialog_w.saturating_sub(meta_str.len() + 6));
            execute!(out, Print(&padded), SetForegroundColor(CColor::DarkGrey), Print(&meta_str))?;
        }
    }

    execute!(out, ResetColor)?;
    Ok(())
}

fn draw_outline(
    app: &App,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    let dialog_w = (width * 4 / 5).clamp(40, 80).min(width);
    let dialog_h = (height * 3 / 4).clamp(8, 16).min(height);
    let start_x = (width.saturating_sub(dialog_w)) / 2;
    let start_y = (height.saturating_sub(dialog_h)) / 2;

    for y in 0..dialog_h {
        execute!(out, MoveTo(start_x as u16, (start_y + y) as u16))?;
        execute!(out, Print(" ".repeat(dialog_w)))?;
    }

    execute!(out, MoveTo((start_x + 2) as u16, (start_y + 1) as u16), SetForegroundColor(CColor::White), SetAttribute(Attribute::Bold))?;
    execute!(out, Print("Outline"))?;
    execute!(out, SetAttribute(Attribute::Reset))?;

    execute!(out, MoveTo((start_x + 2) as u16, (start_y + 2) as u16), SetForegroundColor(CColor::DarkGrey))?;
    execute!(out, Print("─".repeat(dialog_w.saturating_sub(4))))?;

    let list_h = dialog_h.saturating_sub(4);
    for (i, h) in app.headings.iter().take(list_h).enumerate() {
        let is_selected = i == app.outline_selected;
        let row_y = start_y + 3 + i;
        execute!(out, MoveTo((start_x + 2) as u16, row_y as u16))?;

        let indent = "  ".repeat(h.level.saturating_sub(1));
        if is_selected {
            execute!(out, SetForegroundColor(CColor::Cyan), SetAttribute(Attribute::Bold))?;
            execute!(out, Print(format!("> {indent}{}", h.text)))?;
            execute!(out, SetAttribute(Attribute::Reset))?;
        } else {
            execute!(out, SetForegroundColor(CColor::White))?;
            execute!(out, Print(format!("  {indent}{}", h.text)))?;
        }
    }

    execute!(out, ResetColor)?;
    Ok(())
}

fn draw_backlinks(
    app: &App,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    let dialog_w = (width * 4 / 5).clamp(40, 80).min(width);
    let dialog_h = (height * 3 / 4).clamp(8, 16).min(height);
    let start_x = (width.saturating_sub(dialog_w)) / 2;
    let start_y = (height.saturating_sub(dialog_h)) / 2;

    for y in 0..dialog_h {
        execute!(out, MoveTo(start_x as u16, (start_y + y) as u16))?;
        execute!(out, Print(" ".repeat(dialog_w)))?;
    }

    execute!(out, MoveTo((start_x + 2) as u16, (start_y + 1) as u16), SetForegroundColor(CColor::White), SetAttribute(Attribute::Bold))?;
    execute!(out, Print(format!("Backlinks · {}", app.current_title)))?;
    execute!(out, SetAttribute(Attribute::Reset))?;

    execute!(out, MoveTo((start_x + 2) as u16, (start_y + 2) as u16), SetForegroundColor(CColor::DarkGrey))?;
    execute!(out, Print("─".repeat(dialog_w.saturating_sub(4))))?;

    let list_h = dialog_h.saturating_sub(4);
    for (i, b) in app.backlinks.iter().take(list_h).enumerate() {
        let is_selected = i == app.backlinks_selected;
        let row_y = start_y + 3 + i;
        execute!(out, MoveTo((start_x + 2) as u16, row_y as u16))?;

        if is_selected {
            execute!(out, SetForegroundColor(CColor::Cyan), SetAttribute(Attribute::Bold))?;
            execute!(out, Print(format!("> {} ({}:{})", b.from_title, b.from_wiki, b.from_relative.display())))?;
            execute!(out, SetAttribute(Attribute::Reset))?;
        } else {
            execute!(out, SetForegroundColor(CColor::White))?;
            execute!(out, Print(format!("  {} ({}:{})", b.from_title, b.from_wiki, b.from_relative.display())))?;
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

    for y in 0..dialog_h {
        execute!(out, MoveTo(start_x as u16, (start_y + y) as u16))?;
        execute!(out, Print(" ".repeat(dialog_w)))?;
    }

    let help_lines = [
        "TerminalWiki Keybindings",
        "──────────────────────────────────────────",
        "j / Down      Scroll down",
        "k / Up        Scroll up",
        "Ctrl+d / u    Half-page down / up",
        "g / G         Jump to top / bottom",
        "Tab / S-Tab   Cycle through page links",
        "Enter         Follow selected link",
        "f / Ctrl+p    Open fuzzy finder",
        "o             Document outline",
        "b             Toggle backlinks pane",
        "e             Edit current page in $EDITOR",
        "Ctrl+o        Go back in navigation history",
        "Ctrl+i        Go forward in history",
        "?             Toggle this help modal",
        "q / Esc       Quit / Close modal",
    ];

    for (i, line) in help_lines.iter().enumerate() {
        if 1 + i < dialog_h {
            execute!(out, MoveTo((start_x + 2) as u16, (start_y + 1 + i) as u16), SetForegroundColor(CColor::White))?;
            execute!(out, Print(line))?;
        }
    }

    execute!(out, ResetColor)?;
    Ok(())
}

fn draw_search_bar(
    app: &App,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    execute!(out, MoveTo(0, (height - 2) as u16), SetForegroundColor(CColor::Yellow))?;
    let prompt = format!(" /{}█", app.in_page_query);
    let pad = width.saturating_sub(display_width(&prompt));
    execute!(out, Print(&prompt), Print(" ".repeat(pad)), ResetColor)?;
    Ok(())
}
