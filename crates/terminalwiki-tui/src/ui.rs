//! Screen rendering and terminal-native inline layout for TUI (spec §40-§53).

use std::io::{stdout, Write};

use crossterm::cursor::MoveTo;
use crossterm::execute;
use crossterm::style::{
    Attribute, Color as CColor, Print, ResetColor, SetAttribute, SetForegroundColor,
};
use crossterm::terminal::{size as terminal_size, Clear, ClearType};
use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::unicode::{display_width, pad_display_width, truncate_display_width};

use crate::app::{App, Mode};

pub fn draw(app: &App) -> std::io::Result<()> {
    let (cols, rows) = terminal_size()?;
    let width = cols as usize;
    let height = rows as usize;

    if width < 10 || height < 4 {
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

    // ─── 2. Viewport / Inline Views ───────────────────────────────────────────────
    match app.mode {
        Mode::Finder => {
            draw_inline_finder(app, width, height, &mut out)?;
        }
        Mode::Outline => {
            draw_inline_outline(app, width, height, &mut out)?;
        }
        Mode::Backlinks => {
            draw_inline_backlinks(app, width, height, &mut out)?;
        }
        Mode::Help => {
            draw_full_help(width, height, &mut out)?;
        }
        Mode::Graph => {
            draw_graph_view(app, width, height, &mut out)?;
        }
        Mode::Normal | Mode::InPageSearch | Mode::Command if app.page_missing => {
            draw_missing_page(app, width, &mut out)?;
        }
        Mode::Normal | Mode::InPageSearch | Mode::Command => {
            draw_content_viewport(app, width, height, &mut out)?;
        }
    }

    // ─── 3. Bottom Bar / Footer ───────────────────────────────────────────────────
    execute!(out, MoveTo(0, (height - 1) as u16))?;
    match app.mode {
        Mode::Command => {
            execute!(
                out,
                SetForegroundColor(CColor::Cyan),
                Print(format!(":{}█", app.command_input)),
                ResetColor
            )?;
        }
        Mode::InPageSearch => {
            execute!(
                out,
                SetForegroundColor(CColor::Yellow),
                Print(format!(" /{}█", app.in_page_query)),
                ResetColor
            )?;
        }
        _ => {
            let left_text = if let Some(ref msg) = app.status_message {
                format!("  {msg}")
            } else {
                format!("  {}", app.current_path.display())
            };
            let right_text = format!("{}  ", app.current_wiki);

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
        }
    }

    out.flush()?;
    Ok(())
}

fn draw_content_viewport(
    app: &App,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    let content_height = height.saturating_sub(3);
    for row in 0..content_height {
        let line_idx = app.scroll + row;
        execute!(out, MoveTo(0, (row + 2) as u16))?;

        if let Some(line) = app.lines.get(line_idx) {
            let mut current_col = 0;
            let mut skipped_cols = 0;

            for span in line {
                if current_col >= width {
                    break;
                }
                let span_text = sanitize_line(&span.text);
                let text_w = display_width(&span_text);

                // Horizontal scroll offset handling
                if skipped_cols + text_w <= app.h_scroll {
                    skipped_cols += text_w;
                    continue;
                }

                let visible_text = if skipped_cols < app.h_scroll {
                    let trim_amount = app.h_scroll - skipped_cols;
                    skipped_cols = app.h_scroll;
                    truncate_display_width(&span_text, text_w.saturating_sub(trim_amount))
                } else {
                    span_text
                };

                let vis_w = display_width(&visible_text);
                if current_col + vis_w > width {
                    let available = width.saturating_sub(current_col);
                    let truncated = truncate_display_width(&visible_text, available);
                    execute!(out, Print(&span.style.apply(&truncated)))?;
                    current_col += available;
                } else {
                    execute!(out, Print(&span.style.apply(&visible_text)))?;
                    current_col += vis_w;
                }
            }
        }
    }
    Ok(())
}

fn draw_inline_finder(
    app: &App,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    // Top inline search prompt
    execute!(
        out,
        MoveTo(0, 2),
        SetForegroundColor(CColor::Cyan),
        Print("  > "),
        SetForegroundColor(CColor::White),
        Print(&app.finder_query),
        Print("█"),
        ResetColor
    )?;
    execute!(
        out,
        MoveTo(0, 3),
        SetForegroundColor(CColor::DarkGrey),
        Print("─".repeat(width)),
        ResetColor
    )?;

    let list_height = height.saturating_sub(5);
    let start_idx = app.finder_selected.saturating_sub(list_height / 2);

    for (i, hit) in app
        .finder_filtered
        .iter()
        .skip(start_idx)
        .take(list_height)
        .enumerate()
    {
        let is_selected = start_idx + i == app.finder_selected;
        execute!(out, MoveTo(0, (4 + i) as u16))?;

        let title = if hit.title.is_empty() {
            hit.relative.to_string_lossy().to_string()
        } else {
            hit.title.clone()
        };
        let meta = format!("{}:{}", hit.wiki, hit.relative.display());
        let meta_w = display_width(&meta);

        if is_selected {
            execute!(
                out,
                SetForegroundColor(CColor::Cyan),
                SetAttribute(Attribute::Bold)
            )?;
            let display = format!("  > {}", title);
            let padded = pad_display_width(&display, width.saturating_sub(meta_w + 4));
            execute!(
                out,
                Print(&padded),
                SetAttribute(Attribute::Reset),
                SetForegroundColor(CColor::DarkGrey),
                Print(format!("{meta}  ")),
                ResetColor
            )?;
        } else {
            execute!(out, SetForegroundColor(CColor::White))?;
            let display = format!("    {}", title);
            let padded = pad_display_width(&display, width.saturating_sub(meta_w + 4));
            execute!(
                out,
                Print(&padded),
                SetForegroundColor(CColor::DarkGrey),
                Print(format!("{meta}  ")),
                ResetColor
            )?;
        }
    }

    Ok(())
}

fn draw_inline_outline(
    app: &App,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    execute!(
        out,
        MoveTo(0, 2),
        SetForegroundColor(CColor::White),
        SetAttribute(Attribute::Bold),
        Print("  Outline"),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;
    execute!(
        out,
        MoveTo(0, 3),
        SetForegroundColor(CColor::DarkGrey),
        Print("─".repeat(width)),
        ResetColor
    )?;

    let list_height = height.saturating_sub(5);
    for (i, h) in app.headings.iter().take(list_height).enumerate() {
        let is_selected = i == app.outline_selected;
        execute!(out, MoveTo(0, (4 + i) as u16))?;

        let indent = "  ".repeat(h.level.saturating_sub(1));
        if is_selected {
            execute!(
                out,
                SetForegroundColor(CColor::Cyan),
                SetAttribute(Attribute::Bold)
            )?;
            execute!(
                out,
                Print(format!("  > {indent}{}", h.text)),
                SetAttribute(Attribute::Reset)
            )?;
        } else {
            execute!(out, SetForegroundColor(CColor::White))?;
            execute!(out, Print(format!("    {indent}{}", h.text)))?;
        }
    }

    execute!(out, ResetColor)?;
    Ok(())
}

fn draw_inline_backlinks(
    app: &App,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    execute!(
        out,
        MoveTo(0, 2),
        SetForegroundColor(CColor::White),
        SetAttribute(Attribute::Bold),
        Print(format!("  Backlinks · {}", app.current_title)),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;
    execute!(
        out,
        MoveTo(0, 3),
        SetForegroundColor(CColor::DarkGrey),
        Print("─".repeat(width)),
        ResetColor
    )?;

    let list_height = height.saturating_sub(5);
    for (i, b) in app.backlinks.iter().take(list_height).enumerate() {
        let is_selected = i == app.backlinks_selected;
        execute!(out, MoveTo(0, (4 + i) as u16))?;

        if is_selected {
            execute!(
                out,
                SetForegroundColor(CColor::Cyan),
                SetAttribute(Attribute::Bold)
            )?;
            execute!(
                out,
                Print(format!(
                    "  > {} ({}:{})",
                    b.from_title,
                    b.from_wiki,
                    b.from_relative.display()
                )),
                SetAttribute(Attribute::Reset)
            )?;
        } else {
            execute!(out, SetForegroundColor(CColor::White))?;
            execute!(
                out,
                Print(format!(
                    "    {} ({}:{})",
                    b.from_title,
                    b.from_wiki,
                    b.from_relative.display()
                ))
            )?;
        }
    }

    execute!(out, ResetColor)?;
    Ok(())
}

fn draw_full_help(width: usize, height: usize, out: &mut std::io::Stdout) -> std::io::Result<()> {
    execute!(
        out,
        MoveTo(0, 2),
        SetForegroundColor(CColor::White),
        SetAttribute(Attribute::Bold),
        Print("  TerminalWiki Keybindings"),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;
    execute!(
        out,
        MoveTo(0, 3),
        SetForegroundColor(CColor::DarkGrey),
        Print("─".repeat(width)),
        ResetColor
    )?;

    let help_lines = [
        ("j / Down", "Scroll down one line"),
        ("k / Up", "Scroll up one line"),
        ("h / l", "Scroll left / right"),
        ("Ctrl+d / u", "Half-page down / up"),
        ("Home / G", "Jump to top / bottom"),
        ("Tab / S-Tab", "Cycle through links"),
        ("Enter", "Follow selected link / item"),
        ("f / Ctrl+p", "Fuzzy page finder"),
        ("o", "Document outline"),
        ("b", "Backlinks list"),
        ("g", "Local knowledge graph"),
        (":", "Command palette"),
        ("e", "Edit current page in $EDITOR"),
        ("Ctrl+o / i", "Navigate backward / forward"),
        ("?", "Toggle this help view"),
        ("q / Esc", "Quit / Return to document"),
        ("", ""),
        ("In the graph", ""),
        ("j / k / Tab", "Select node"),
        ("Enter", "Open selected node"),
        ("+ / -", "Increase / decrease depth"),
        ("r", "Rebuild graph from index"),
        ("Esc", "Back to article"),
    ];

    let view_height = height.saturating_sub(5);
    for (i, (key, desc)) in help_lines.iter().take(view_height).enumerate() {
        execute!(out, MoveTo(0, (4 + i) as u16))?;
        execute!(
            out,
            SetForegroundColor(CColor::Cyan),
            Print(format!("  {:<14}", key)),
            SetForegroundColor(CColor::White),
            Print(desc),
            ResetColor
        )?;
    }

    Ok(())
}

/// Draws the interactive local graph (spec items 41-45).
///
/// The canvas is drawn plainly first, then the selected node's label is
/// reprinted in place with accent styling. Restyling in place — rather than
/// searching the rendered rows for the title — is what makes duplicate and
/// truncated labels highlight correctly.
fn draw_graph_view(
    app: &App,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    let Some(view) = app.graph_view.as_ref() else {
        return Ok(());
    };
    let Some(graph) = app.graph_cache.as_ref() else {
        return Ok(());
    };

    let header = format!(
        "  Graph · {} · depth {}",
        sanitize_line(&app.current_title),
        view.depth
    );
    execute!(
        out,
        MoveTo(0, 2),
        SetForegroundColor(CColor::White),
        SetAttribute(Attribute::Bold),
        Print(truncate_display_width(&header, width)),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;
    execute!(
        out,
        MoveTo(0, 3),
        SetForegroundColor(CColor::DarkGrey),
        Print("─".repeat(width)),
        ResetColor
    )?;

    // Reserve the header (4 rows) and a summary line above the footer.
    const CANVAS_TOP: usize = 4;
    let canvas_height = height.saturating_sub(CANVAS_TOP + 2);
    if canvas_height == 0 || width == 0 {
        return Ok(());
    }

    let rendered =
        terminalwiki_graph::render_graph(graph, &view.sub, &view.pos, width, canvas_height);

    for (i, line) in rendered.lines.iter().enumerate() {
        execute!(
            out,
            MoveTo(0, (CANVAS_TOP + i) as u16),
            SetForegroundColor(CColor::DarkGrey),
            Print(sanitize_line(line)),
            ResetColor
        )?;
    }

    // Highlight the selected node: accent + bold, no box (old Gate 7.6).
    if let Some(selected_node) = view.selected_node() {
        if let Some(p) = rendered.labels.iter().find(|p| p.node == selected_node) {
            execute!(
                out,
                MoveTo(p.marker_col as u16, (CANVAS_TOP + p.row) as u16),
                SetForegroundColor(CColor::Cyan),
                SetAttribute(Attribute::Bold),
                Print("◉"),
                SetAttribute(Attribute::Reset),
                ResetColor
            )?;
            if p.width > 0 {
                execute!(
                    out,
                    MoveTo(p.col as u16, (CANVAS_TOP + p.row) as u16),
                    SetForegroundColor(CColor::Cyan),
                    SetAttribute(Attribute::Bold),
                    SetAttribute(Attribute::Reverse),
                    Print(sanitize_line(&p.text)),
                    SetAttribute(Attribute::Reset),
                    ResetColor
                )?;
            }
        }
    }

    // Summary line: what is shown, and the selected node's identity.
    let selected_title = view
        .selected_node()
        .and_then(|n| graph.node(n))
        .map(|n| n.title.clone())
        .unwrap_or_default();
    let capped_note = if view.capped {
        format!(" (capped at {})", view.sub.nodes.len())
    } else {
        String::new()
    };
    let summary = format!(
        "  {} nodes{}   ▸ {}",
        view.sub.nodes.len(),
        capped_note,
        sanitize_line(&selected_title)
    );
    execute!(
        out,
        MoveTo(0, (height - 2) as u16),
        SetForegroundColor(CColor::DarkGrey),
        Print(truncate_display_width(&summary, width)),
        ResetColor
    )?;

    Ok(())
}

/// Shown when the open page was deleted on disk (spec Gate 2.15).
///
/// Deliberately plain text with no box: the article area keeps its typography
/// even when it has nothing to show.
fn draw_missing_page(app: &App, width: usize, out: &mut std::io::Stdout) -> std::io::Result<()> {
    let path = sanitize_line(&app.current_path.to_string_lossy());
    let lines = [
        String::new(),
        "  This page was removed from disk.".to_string(),
        String::new(),
        format!(
            "  {}",
            truncate_display_width(&path, width.saturating_sub(4))
        ),
        String::new(),
        "  b   back to the previous page".to_string(),
        "  r   retry loading it".to_string(),
    ];

    for (i, line) in lines.iter().enumerate() {
        execute!(
            out,
            MoveTo(0, (i + 2) as u16),
            SetForegroundColor(CColor::DarkGrey),
            Print(truncate_display_width(line, width)),
            ResetColor
        )?;
    }
    Ok(())
}
