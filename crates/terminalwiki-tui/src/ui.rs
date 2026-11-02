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

use crate::app::{App, Mode, SidebarEntry, SidebarMode, ViewLayout};

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
    let star = if app.bookmarks.contains(&app.current_path) {
        "★ "
    } else {
        ""
    };
    let title_display = if path_str.is_empty() {
        format!("{star}{} · Home", app.current_wiki)
    } else {
        format!("{star}{} / {}", app.current_wiki, path_str)
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
    let in_article_mode = matches!(
        app.mode,
        Mode::Normal
            | Mode::InPageSearch
            | Mode::Command
            | Mode::Sidebar
            | Mode::NewPage
            | Mode::TagPrompt
    );

    let show_left_sidebar = in_article_mode
        && width >= 50
        && match app.layout_mode {
            ViewLayout::Zen => false,
            ViewLayout::Wikipedia => true,
            ViewLayout::Standard => app.sidebar_visible,
        };
    let sidebar_width = if show_left_sidebar {
        (width / 4).clamp(20, 30)
    } else {
        0
    };

    let inspector_width =
        if in_article_mode && app.layout_mode == ViewLayout::Wikipedia && width >= 90 {
            (width / 4).clamp(22, 32)
        } else {
            0
        };

    match app.mode {
        Mode::Finder => {
            draw_inline_finder(app, width, height, &mut out)?;
        }
        Mode::WikiSearch => {
            draw_inline_wiki_search(app, width, height, &mut out)?;
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
        Mode::Normal
        | Mode::InPageSearch
        | Mode::Command
        | Mode::Sidebar
        | Mode::NewPage
        | Mode::TagPrompt
            if app.page_missing =>
        {
            if sidebar_width > 0 {
                draw_sidebar(app, sidebar_width, height, &mut out)?;
            }
            draw_missing_page(
                app,
                sidebar_width,
                width.saturating_sub(sidebar_width + inspector_width),
                &mut out,
            )?;
        }
        Mode::Normal
        | Mode::InPageSearch
        | Mode::Command
        | Mode::Sidebar
        | Mode::NewPage
        | Mode::TagPrompt => {
            if sidebar_width > 0 {
                draw_sidebar(app, sidebar_width, height, &mut out)?;
            }
            let avail_w = width.saturating_sub(sidebar_width + inspector_width);
            let (content_x, content_w) = if app.layout_mode == ViewLayout::Zen && avail_w > 86 {
                let target_w = 80;
                let margin = (avail_w - target_w) / 2;
                (sidebar_width + margin, target_w)
            } else {
                (sidebar_width, avail_w)
            };
            draw_content_viewport(app, content_x, content_w, height, &mut out)?;
            if inspector_width > 0 {
                draw_wikipedia_inspector(
                    app,
                    width.saturating_sub(inspector_width),
                    inspector_width,
                    height,
                    &mut out,
                )?;
            }
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
            let match_info = if app.in_page_query.is_empty() {
                "type to search in page".to_string()
            } else if app.search_matches.is_empty() {
                "0 matches".to_string()
            } else {
                format!(
                    "{}/{} matches (↑/↓/Tab cycle, Enter jump)",
                    app.search_match_idx + 1,
                    app.search_matches.len()
                )
            };
            let prompt = format!(" /{}█   {}", app.in_page_query, match_info);
            execute!(
                out,
                SetForegroundColor(CColor::Yellow),
                Print(truncate_display_width(&prompt, width)),
                ResetColor
            )?;
        }
        Mode::TagPrompt => {
            let prompt = format!(" #{}█   Enter add tag to page   Esc cancel", app.tag_input);
            execute!(
                out,
                SetForegroundColor(CColor::Cyan),
                Print(truncate_display_width(&prompt, width)),
                ResetColor
            )?;
        }
        Mode::NewPage => {
            let prompt = format!(
                " + {}█   Tab template: {}   Enter create   Ctrl+E edit in $EDITOR   Esc cancel",
                app.new_page_name,
                app.new_page_template.label()
            );
            execute!(
                out,
                SetForegroundColor(CColor::Yellow),
                Print(truncate_display_width(&prompt, width)),
                ResetColor
            )?;
        }
        Mode::Sidebar => {
            let left_text = if let Some(ref msg) = app.status_message {
                format!("  {msg}")
            } else {
                "  s hide · v tree/list · j/k select · Enter open · Esc read · Ctrl+P quick bar"
                    .to_string()
            };
            let right_text = format!("{}  ", app.current_wiki);
            let left_w = display_width(&left_text);
            let right_w = display_width(&right_text);
            let foot_pad = width.saturating_sub(left_w + right_w);
            execute!(
                out,
                SetForegroundColor(CColor::DarkGrey),
                Print(truncate_display_width(
                    &left_text,
                    width.saturating_sub(right_w)
                )),
                Print(" ".repeat(foot_pad)),
                Print(&right_text),
                ResetColor
            )?;
        }
        _ => {
            let left_text = if let Some(ref msg) = app.status_message {
                format!("  {msg}")
            } else {
                format!(
                    "  {}  [{} · Ctrl+P quick bar · Ctrl+F find · Ctrl+W search · Q quicknote]",
                    app.current_path.display(),
                    app.layout_mode.label()
                )
            };
            let right_text = format!("{}  ", app.current_wiki);

            let left_w = display_width(&left_text);
            let right_w = display_width(&right_text);
            let foot_pad = width.saturating_sub(left_w + right_w);

            execute!(
                out,
                SetForegroundColor(CColor::DarkGrey),
                Print(truncate_display_width(
                    &left_text,
                    width.saturating_sub(right_w)
                )),
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
    x: usize,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    let content_height = height.saturating_sub(3);
    let active_link = app.selected_link_idx.and_then(|idx| app.links.get(idx));
    let search_q_lower = app.in_page_query.trim().to_lowercase();

    for row in 0..content_height {
        let line_idx = app.scroll + row;
        execute!(out, MoveTo(x as u16, (row + 2) as u16))?;

        if let Some(line) = app.lines.get(line_idx) {
            let mut current_col = 0;
            let mut skipped_cols = 0;
            let mut abs_col = 0;
            let line_has_search_match =
                !search_q_lower.is_empty() && app.search_matches.binary_search(&line_idx).is_ok();

            for span in line {
                if current_col >= width {
                    break;
                }
                let span_text = sanitize_line(&span.text);
                let text_w = display_width(&span_text);
                let span_start = abs_col;
                let span_end = abs_col + text_w;
                abs_col = span_end;

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

                let mut style = span.style.clone();
                if let Some(link) = active_link {
                    if link.line == line_idx
                        && span_start < link.end_column
                        && span_end > link.start_column
                    {
                        style = style.bold().reverse();
                    }
                }
                if line_has_search_match && visible_text.to_lowercase().contains(&search_q_lower) {
                    style = style.bold().reverse();
                }

                let vis_w = display_width(&visible_text);
                if current_col + vis_w > width {
                    let available = width.saturating_sub(current_col);
                    let truncated = truncate_display_width(&visible_text, available);
                    execute!(out, Print(&style.apply(&truncated)))?;
                    current_col += available;
                } else {
                    execute!(out, Print(&style.apply(&visible_text)))?;
                    current_col += vis_w;
                }
            }
        }
    }
    Ok(())
}

fn draw_sidebar(
    app: &App,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    let entries = app.sidebar_entries();
    let header_label = match app.sidebar_mode {
        SidebarMode::Flat => format!("  Pages ({})", entries.len()),
        SidebarMode::Tree => format!("  Vault Tree ({})", entries.len()),
        SidebarMode::Outline => format!("  Outline ({})", entries.len()),
        SidebarMode::Tags => format!("  Tags ({})", entries.len()),
        SidebarMode::Bookmarks => format!("  Bookmarks ({})", entries.len()),
    };
    execute!(
        out,
        MoveTo(0, 2),
        SetForegroundColor(CColor::White),
        SetAttribute(Attribute::Bold),
        Print(pad_display_width(&header_label, width.saturating_sub(1))),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;
    execute!(
        out,
        MoveTo(0, 3),
        SetForegroundColor(CColor::DarkGrey),
        Print("─".repeat(width.saturating_sub(1))),
        ResetColor
    )?;

    let list_top = 4usize;
    let list_height = height.saturating_sub(list_top + 2);
    let mut start = app.sidebar_selected.saturating_sub(list_height / 2);
    if start + list_height > entries.len() {
        start = entries.len().saturating_sub(list_height);
    }

    for (offset, entry) in entries.iter().skip(start).take(list_height).enumerate() {
        let index = start + offset;
        let selected = app.mode == Mode::Sidebar && index == app.sidebar_selected;
        let mut is_current = false;
        let display_entry = match entry {
            SidebarEntry::Page {
                wiki,
                relative,
                title,
            } => {
                let current = wiki == &app.current_wiki && relative == &app.current_path;
                is_current = current;
                let marker = if current { "▸ " } else { "  " };
                match app.sidebar_mode {
                    SidebarMode::Flat => format!("{marker}{title}"),
                    SidebarMode::Tree => {
                        let parent = relative
                            .parent()
                            .and_then(|p| (!p.as_os_str().is_empty()).then(|| p.to_string_lossy()));
                        let stem = relative
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| title.clone());
                        if let Some(dir) = parent {
                            format!("{marker}📁 {dir}/{stem}")
                        } else {
                            format!("{marker}📄 {stem}")
                        }
                    }
                    SidebarMode::Bookmarks => format!("{marker}★ {title}"),
                    _ => format!("{marker}{title}"),
                }
            }
            SidebarEntry::Heading { title, level, .. } => {
                let indent = "  ".repeat(level.saturating_sub(1));
                format!("{indent}§ {title}")
            }
            SidebarEntry::Tag { tag, count } => {
                format!("  🏷 #{tag} ({count})")
            }
        };
        let line = truncate_display_width(&display_entry, width.saturating_sub(2));

        execute!(out, MoveTo(0, (list_top + offset) as u16))?;
        if selected {
            execute!(
                out,
                SetForegroundColor(CColor::Cyan),
                SetAttribute(Attribute::Bold),
                SetAttribute(Attribute::Reverse),
                Print(pad_display_width(&line, width.saturating_sub(1))),
                SetAttribute(Attribute::Reset),
                ResetColor
            )?;
        } else if is_current {
            execute!(
                out,
                SetForegroundColor(CColor::White),
                SetAttribute(Attribute::Bold),
                Print(line),
                SetAttribute(Attribute::Reset),
                ResetColor
            )?;
        } else {
            execute!(
                out,
                SetForegroundColor(CColor::Grey),
                Print(line),
                ResetColor
            )?;
        }
    }

    // A subtle divider keeps the article column visually separate.
    for row in 2..height.saturating_sub(1) {
        execute!(
            out,
            MoveTo((width.saturating_sub(1)) as u16, row as u16),
            SetForegroundColor(CColor::DarkGrey),
            Print("│"),
            ResetColor
        )?;
    }

    Ok(())
}

fn draw_wikipedia_inspector(
    app: &App,
    x: usize,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    // Left border of inspector pane
    for row in 2..height.saturating_sub(1) {
        execute!(
            out,
            MoveTo(x as u16, row as u16),
            SetForegroundColor(CColor::DarkGrey),
            Print("│"),
            ResetColor
        )?;
    }

    let inner_x = (x + 2) as u16;
    let inner_w = width.saturating_sub(3);
    if inner_w < 8 {
        return Ok(());
    }

    execute!(
        out,
        MoveTo(inner_x, 2),
        SetForegroundColor(CColor::White),
        SetAttribute(Attribute::Bold),
        Print(truncate_display_width("Contents & Links", inner_w)),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;
    execute!(
        out,
        MoveTo((x + 1) as u16, 3),
        SetForegroundColor(CColor::DarkGrey),
        Print("─".repeat(width.saturating_sub(1))),
        ResetColor
    )?;

    let max_rows = height.saturating_sub(6);
    let mut row_cursor = 4usize;

    // 1. Outline / TOC section
    if !app.headings.is_empty() && row_cursor < 4 + max_rows {
        execute!(
            out,
            MoveTo(inner_x, row_cursor as u16),
            SetForegroundColor(CColor::Cyan),
            SetAttribute(Attribute::Bold),
            Print(truncate_display_width(
                &format!("§ Outline ({})", app.headings.len()),
                inner_w
            )),
            SetAttribute(Attribute::Reset),
            ResetColor
        )?;
        row_cursor += 1;

        for h in app.headings.iter().take(max_rows / 2) {
            if row_cursor >= 4 + max_rows {
                break;
            }
            let indent = " ".repeat(h.level.saturating_sub(1));
            let line = truncate_display_width(&format!("{indent}• {}", h.text), inner_w);
            execute!(
                out,
                MoveTo(inner_x, row_cursor as u16),
                SetForegroundColor(CColor::Grey),
                Print(line),
                ResetColor
            )?;
            row_cursor += 1;
        }
        row_cursor += 1;
    }

    // 2. Backlinks section
    if row_cursor < 4 + max_rows {
        execute!(
            out,
            MoveTo(inner_x, row_cursor as u16),
            SetForegroundColor(CColor::Cyan),
            SetAttribute(Attribute::Bold),
            Print(truncate_display_width(
                &format!("← Backlinks ({})", app.backlinks.len()),
                inner_w
            )),
            SetAttribute(Attribute::Reset),
            ResetColor
        )?;
        row_cursor += 1;

        if app.backlinks.is_empty() {
            if row_cursor < 4 + max_rows {
                execute!(
                    out,
                    MoveTo(inner_x, row_cursor as u16),
                    SetForegroundColor(CColor::DarkGrey),
                    Print(truncate_display_width("(none)", inner_w)),
                    ResetColor
                )?;
                row_cursor += 1;
            }
        } else {
            for b in &app.backlinks {
                if row_cursor >= 4 + max_rows {
                    break;
                }
                let line = truncate_display_width(&format!("• {}", b.from_title), inner_w);
                execute!(
                    out,
                    MoveTo(inner_x, row_cursor as u16),
                    SetForegroundColor(CColor::White),
                    Print(line),
                    ResetColor
                )?;
                row_cursor += 1;
            }
        }
        row_cursor += 1;
    }

    // 3. Outgoing links section
    if row_cursor < 4 + max_rows {
        execute!(
            out,
            MoveTo(inner_x, row_cursor as u16),
            SetForegroundColor(CColor::Cyan),
            SetAttribute(Attribute::Bold),
            Print(truncate_display_width(
                &format!("→ Links ({})", app.links.len()),
                inner_w
            )),
            SetAttribute(Attribute::Reset),
            ResetColor
        )?;
        row_cursor += 1;

        for (idx, link) in app.links.iter().enumerate() {
            if row_cursor >= 4 + max_rows {
                break;
            }
            let active = app.selected_link_idx == Some(idx);
            let prefix = if idx < 9 {
                format!("[{}] ", idx + 1)
            } else {
                "• ".to_string()
            };
            let line = truncate_display_width(&format!("{prefix}{}", link.label), inner_w);
            execute!(
                out,
                MoveTo(inner_x, row_cursor as u16),
                SetForegroundColor(if active { CColor::Cyan } else { CColor::Grey }),
                Print(line),
                ResetColor
            )?;
            row_cursor += 1;
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
    let hint = if app.finder_is_command_mode() {
        "Command Palette (Enter to run · Esc to close)  "
    } else {
        "Quick Bar (#tag filter · > commands)  "
    };
    let left_prompt = format!("  > {}█", app.finder_query);
    let left_w = display_width(&left_prompt);
    let hint_w = display_width(hint);
    let pad = width.saturating_sub(left_w + hint_w);

    execute!(
        out,
        MoveTo(0, 2),
        SetForegroundColor(CColor::Cyan),
        Print("  > "),
        SetForegroundColor(CColor::White),
        Print(&app.finder_query),
        Print("█"),
        Print(" ".repeat(pad)),
        SetForegroundColor(CColor::DarkGrey),
        Print(hint),
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

    if app.finder_is_command_mode() {
        for (i, cmd) in app
            .finder_commands
            .iter()
            .skip(start_idx)
            .take(list_height)
            .enumerate()
        {
            let is_selected = start_idx + i == app.finder_selected;
            execute!(out, MoveTo(0, (4 + i) as u16))?;

            let meta = format!("[{}]", cmd.shortcut);
            let meta_w = display_width(&meta);
            let label = format!("{} — {}", cmd.title, cmd.description);

            if is_selected {
                let display = truncate_display_width(
                    &format!("  > {label}"),
                    width.saturating_sub(meta_w + 4),
                );
                let padded = pad_display_width(&display, width.saturating_sub(meta_w + 4));
                execute!(
                    out,
                    SetForegroundColor(CColor::Cyan),
                    SetAttribute(Attribute::Bold),
                    Print(&padded),
                    SetAttribute(Attribute::Reset),
                    SetForegroundColor(CColor::Yellow),
                    Print(format!("{meta}  ")),
                    ResetColor
                )?;
            } else {
                let display = truncate_display_width(
                    &format!("    {label}"),
                    width.saturating_sub(meta_w + 4),
                );
                let padded = pad_display_width(&display, width.saturating_sub(meta_w + 4));
                execute!(
                    out,
                    SetForegroundColor(CColor::White),
                    Print(&padded),
                    SetForegroundColor(CColor::DarkGrey),
                    Print(format!("{meta}  ")),
                    ResetColor
                )?;
            }
        }
        return Ok(());
    }

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

fn draw_inline_wiki_search(
    app: &App,
    width: usize,
    height: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
    let right_hint = format!("{} hits (Enter to open)  ", app.wiki_search_hits.len());
    let left_prompt = format!("  🔎 Wiki Search: {}█", app.wiki_search_query);
    let pad = width.saturating_sub(display_width(&left_prompt) + display_width(&right_hint));

    execute!(
        out,
        MoveTo(0, 2),
        SetForegroundColor(CColor::Yellow),
        SetAttribute(Attribute::Bold),
        Print("  🔎 Wiki Search: "),
        SetAttribute(Attribute::Reset),
        SetForegroundColor(CColor::White),
        Print(&app.wiki_search_query),
        Print("█"),
        Print(" ".repeat(pad)),
        SetForegroundColor(CColor::DarkGrey),
        Print(&right_hint),
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
    let start_idx = app.wiki_search_selected.saturating_sub(list_height / 2);

    for (i, hit) in app
        .wiki_search_hits
        .iter()
        .skip(start_idx)
        .take(list_height)
        .enumerate()
    {
        let is_selected = start_idx + i == app.wiki_search_selected;
        execute!(out, MoveTo(0, (4 + i) as u16))?;

        let meta = format!(
            "{}:{}:{}",
            hit.wiki,
            hit.relative.display(),
            hit.line_number
        );
        let meta_w = display_width(&meta);
        let max_body_w = width.saturating_sub(meta_w + 6);
        let combined = if hit.snippet.is_empty() {
            hit.title.clone()
        } else {
            format!("{} — {}", hit.title, hit.snippet)
        };
        let prefix = if is_selected { "  > " } else { "    " };
        let display = truncate_display_width(&format!("{prefix}{combined}"), max_body_w);
        let padded = pad_display_width(&display, max_body_w);

        if is_selected {
            execute!(
                out,
                SetForegroundColor(CColor::Cyan),
                SetAttribute(Attribute::Bold),
                Print(&padded),
                SetAttribute(Attribute::Reset),
                SetForegroundColor(CColor::Yellow),
                Print(format!("  {meta}  ")),
                ResetColor
            )?;
        } else {
            execute!(
                out,
                SetForegroundColor(CColor::White),
                Print(&padded),
                SetForegroundColor(CColor::DarkGrey),
                Print(format!("  {meta}  ")),
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
        ("j / k / Up / Down", "Scroll down / up one line"),
        ("h / l / Left / Right", "Scroll left / right"),
        ("Ctrl+d / u", "Half-page down / up"),
        ("Home / G", "Jump to top / bottom"),
        (
            "Tab / S-Tab / ] / [",
            "Cycle through links (highlighted in article)",
        ),
        (
            "1..9 / Enter",
            "Quick-jump link 1..9 / follow selected link",
        ),
        (
            "Ctrl+p / f",
            "VS Code Quick Bar (pages, #tag filter, > commands)",
        ),
        (">", "Open Quick Bar directly in Command Palette mode"),
        ("Ctrl+f / /", "Live in-page search (F3 / Ctrl+g next match)"),
        (
            "Ctrl+w",
            "Wiki-wide full-text search (words & sentence parts)",
        ),
        ("Ctrl+n / Q", "Quick Note in QuickNotes/ (opens nvim/vim)"),
        ("n / T", "New page / new todo list (Ctrl+e opens editor)"),
        ("t", "Add #tag to current page"),
        ("e", "Edit current page in $EDITOR (nvim/vim)"),
        ("z", "Toggle Zen distraction-free reading mode"),
        (
            "w",
            "Toggle Wikipedia 3-pane layout (Vault + Article + TOC)",
        ),
        ("s / v", "Toggle sidebar / switch Vault Tree vs Flat list"),
        (
            "o / b / g",
            "Document outline / Backlinks / Knowledge graph",
        ),
        (
            "Ctrl+o / i / Bksp",
            "Navigate backward / forward in history",
        ),
        (": / ? / q", "Command line / Help / Quit"),
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

    let scope_label = if view.is_global {
        "Global Vault Graph".to_string()
    } else {
        format!("depth {}", view.depth)
    };
    let header = format!(
        "  Graph · {} · {} · [+/-] depth · [a] all/local · [Enter] open · [Esc] close",
        sanitize_line(&app.current_title),
        scope_label
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

    // Color-coded nodes and distinct markers by node document kind
    for p in &rendered.labels {
        let (color, glyph) = match p.kind {
            terminalwiki_graph::NodeKind::Page => (CColor::Green, "●"),
            terminalwiki_graph::NodeKind::Latex => (CColor::Magenta, "◆"),
            terminalwiki_graph::NodeKind::Bibtex => (CColor::Yellow, "▲"),
            terminalwiki_graph::NodeKind::Pdf => (CColor::Red, "■"),
            terminalwiki_graph::NodeKind::CodeFile => (CColor::Blue, "λ"),
            terminalwiki_graph::NodeKind::Asset => (CColor::Cyan, "◈"),
            terminalwiki_graph::NodeKind::Wiki => (CColor::White, "★"),
        };
        execute!(
            out,
            MoveTo(p.marker_col as u16, (CANVAS_TOP + p.row) as u16),
            SetForegroundColor(color),
            Print(glyph),
            ResetColor
        )?;
        if p.width > 0 {
            execute!(
                out,
                MoveTo(p.col as u16, (CANVAS_TOP + p.row) as u16),
                SetForegroundColor(CColor::White),
                Print(sanitize_line(&p.text)),
                ResetColor
            )?;
        }
    }

    // Highlight the selected node: accent + bold + reverse
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

    // Summary line: what is shown, node kind, path, and legend.
    let selected_info = view
        .selected_node()
        .and_then(|n| graph.node(n))
        .map(|n| {
            let kind_label = match n.kind {
                terminalwiki_graph::NodeKind::Page => "Page",
                terminalwiki_graph::NodeKind::Latex => "LaTeX",
                terminalwiki_graph::NodeKind::Bibtex => "BibTeX",
                terminalwiki_graph::NodeKind::Pdf => "PDF",
                terminalwiki_graph::NodeKind::CodeFile => "Code",
                terminalwiki_graph::NodeKind::Asset => "Asset",
                terminalwiki_graph::NodeKind::Wiki => "Wiki",
            };
            format!("[{kind_label}] {} ({})", n.title, n.id.relative.display())
        })
        .unwrap_or_default();

    let capped_note = if view.capped {
        format!(" (capped at {})", view.sub.nodes.len())
    } else {
        String::new()
    };
    let summary = format!(
        "  {} nodes{}   ▸ {}   |   ● Note  ◆ LaTeX  ▲ BibTeX  ■ PDF  λ Code",
        view.sub.nodes.len(),
        capped_note,
        sanitize_line(&selected_info)
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
fn draw_missing_page(
    app: &App,
    x: usize,
    width: usize,
    out: &mut std::io::Stdout,
) -> std::io::Result<()> {
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
            MoveTo(x as u16, (i + 2) as u16),
            SetForegroundColor(CColor::DarkGrey),
            Print(truncate_display_width(line, width)),
            ResetColor
        )?;
    }
    Ok(())
}
