//! Markdown parser producing semantic AST blocks and inlines (spec §55, §56).

use pulldown_cmark::{Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};

use crate::document::{Block, Document, Inline};

/// Parses raw markdown text into a semantic `Document` AST.
pub fn parse_markdown(text: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_MATH);
    // Without this, CommonMark parses `[[Heap]]` as a nested link reference and
    // splits it across several `Event::Text` chunks, so no text-level scanner can
    // ever see a complete `[[...]]` span.
    options.insert(Options::ENABLE_WIKILINKS);

    // pulldown-cmark recognizes `$...$`/`$$...$$`, while Obsidian-style notes
    // frequently also use `\(...\)` and `\[...\]`. Normalize only outside
    // fenced and inline code, preserving escaped delimiters verbatim.
    let normalized = normalize_math_delimiters(text);
    let parser = Parser::new_ext(&normalized, options);
    let mut doc = Document::default();

    let mut block_stack: Vec<Vec<Block>> = vec![Vec::new()];
    let mut inline_stack: Vec<Vec<Inline>> = vec![Vec::new()];
    // (destination, wikilink pothole flag) — `None` means an ordinary CommonMark link.
    let mut link_dest_stack: Vec<(String, Option<bool>)> = Vec::new();
    let mut image_dest_stack: Vec<String> = Vec::new();

    let mut current_code_block: Option<Option<String>> = None;
    let mut current_code_content = String::new();
    let mut footnote_stack = Vec::new();
    let mut list_stack: Vec<(bool, u64, Vec<Vec<Block>>)> = Vec::new();

    // Table state
    let mut in_table = false;
    let mut in_table_head = false;
    let mut table_headers: Vec<Vec<Inline>> = Vec::new();
    let mut table_rows: Vec<Vec<Vec<Inline>>> = Vec::new();
    let mut current_row: Vec<Vec<Inline>> = Vec::new();

    for event in parser {
        match event {
            // ─── Code Blocks ─────────────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(l) => {
                        let l_str = l.to_string();
                        if l_str.is_empty() {
                            None
                        } else {
                            Some(l_str)
                        }
                    }
                    pulldown_cmark::CodeBlockKind::Indented => None,
                };
                current_code_block = Some(lang);
                current_code_content.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(lang) = current_code_block.take() {
                    block_stack.last_mut().unwrap().push(Block::CodeBlock {
                        language: lang,
                        content: std::mem::take(&mut current_code_content),
                    });
                }
            }

            // ─── Headings ────────────────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                inline_stack.push(Vec::new());
                let _ = level;
            }
            Event::End(TagEnd::Heading(level)) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                let lvl = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                block_stack
                    .last_mut()
                    .unwrap()
                    .push(Block::Heading(lvl, inlines));
            }

            // ─── Paragraphs ──────────────────────────────────────────────────────
            Event::Start(Tag::Paragraph) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Paragraph) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                if !inlines.is_empty() {
                    block_stack
                        .last_mut()
                        .unwrap()
                        .push(Block::Paragraph(inlines));
                }
            }

            // ─── Block Quotes ────────────────────────────────────────────────────
            Event::Start(Tag::BlockQuote(_)) => {
                block_stack.push(Vec::new());
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                let inner = block_stack.pop().unwrap_or_default();
                block_stack
                    .last_mut()
                    .unwrap()
                    .push(Block::BlockQuote(inner));
            }
            Event::Start(Tag::FootnoteDefinition(name)) => {
                block_stack.push(Vec::new());
                footnote_stack.push(name.to_string());
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                let content = block_stack.pop().unwrap_or_default();
                let name = footnote_stack.pop().unwrap_or_default();
                block_stack
                    .last_mut()
                    .unwrap()
                    .push(Block::FootnoteDefinition { name, content });
            }

            // ─── Lists ───────────────────────────────────────────────────────────
            Event::Start(Tag::List(first_item_number)) => {
                list_stack.push((
                    first_item_number.is_some(),
                    first_item_number.unwrap_or(1),
                    Vec::new(),
                ));
            }
            Event::End(TagEnd::List(_)) => {
                let Some((ordered, start, items)) = list_stack.pop() else {
                    continue;
                };
                block_stack.last_mut().unwrap().push(Block::List {
                    ordered,
                    start,
                    items,
                });
            }
            Event::Start(Tag::Item) => {
                block_stack.push(Vec::new());
                // Tight CommonMark list items do not emit paragraph events.
                // Give such direct inline events an item-local buffer.
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Item) => {
                let mut item_blocks = block_stack.pop().unwrap_or_default();
                let loose_inlines = inline_stack.pop().unwrap_or_default();
                if !loose_inlines.is_empty() {
                    item_blocks.insert(0, Block::Paragraph(loose_inlines));
                }
                if let Some((_, _, items)) = list_stack.last_mut() {
                    items.push(item_blocks);
                }
            }

            // ─── Tables ──────────────────────────────────────────────────────────
            Event::Start(Tag::Table(_)) => {
                in_table = true;
                table_headers.clear();
                table_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                in_table = false;
                block_stack.last_mut().unwrap().push(Block::Table {
                    headers: std::mem::take(&mut table_headers),
                    rows: std::mem::take(&mut table_rows),
                });
            }
            Event::Start(Tag::TableHead) => {
                in_table_head = true;
                current_row.clear();
            }
            Event::End(TagEnd::TableHead) => {
                in_table_head = false;
                table_headers = std::mem::take(&mut current_row);
            }
            Event::Start(Tag::TableRow) => {
                current_row.clear();
            }
            Event::End(TagEnd::TableRow) => {
                if !in_table_head && in_table {
                    table_rows.push(std::mem::take(&mut current_row));
                }
            }
            Event::Start(Tag::TableCell) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::TableCell) => {
                let cell_inlines = inline_stack.pop().unwrap_or_default();
                current_row.push(cell_inlines);
            }

            // ─── Inlines ─────────────────────────────────────────────────────────
            Event::Start(Tag::Emphasis) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Emphasis) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Italic(inlines));
            }
            Event::Start(Tag::Strong) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Strong) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                inline_stack.last_mut().unwrap().push(Inline::Bold(inlines));
            }
            Event::Start(Tag::Strikethrough) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Strikethrough) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Strike(inlines));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) => {
                let pothole = match link_type {
                    LinkType::WikiLink { has_pothole } => Some(has_pothole),
                    _ => None,
                };
                link_dest_stack.push((dest_url.to_string(), pothole));
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Link) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                let (target, pothole) = link_dest_stack.pop().unwrap_or_default();
                let inline = match pothole {
                    // `[[Target|Label]]` — the link body is a distinct display label.
                    Some(true) => Inline::WikiLink {
                        target,
                        label: Some(inlines.iter().map(|i| i.plain_text()).collect::<String>()),
                    },
                    // `[[Target]]` — body and target are the same text.
                    Some(false) => Inline::WikiLink {
                        target,
                        label: None,
                    },
                    None => Inline::Link {
                        target,
                        text: inlines,
                    },
                };
                inline_stack.last_mut().unwrap().push(inline);
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                image_dest_stack.push(dest_url.to_string());
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Image) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                let alt = inlines.iter().map(|i| i.plain_text()).collect::<String>();
                let url = image_dest_stack.pop().unwrap_or_default();
                block_stack
                    .last_mut()
                    .unwrap()
                    .push(Block::Image { alt, url });
            }

            // ─── Text & Code ─────────────────────────────────────────────────────
            Event::Text(t) => {
                if current_code_block.is_some() {
                    current_code_content.push_str(&t);
                } else {
                    parse_inline_extensions(&t, inline_stack.last_mut().unwrap());
                }
            }
            Event::Code(t) => {
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Code(t.to_string()));
            }
            Event::Rule => {
                block_stack.last_mut().unwrap().push(Block::HorizontalRule);
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x] " } else { "[ ] " };
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Text(marker.to_string()));
            }
            Event::FootnoteReference(name) => {
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Footnote(format!("[^{name}]")));
            }
            Event::InlineMath(m) => {
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Math(m.to_string()));
            }
            Event::DisplayMath(m) => {
                block_stack
                    .last_mut()
                    .unwrap()
                    .push(Block::Math(m.to_string()));
            }
            Event::SoftBreak | Event::HardBreak => {
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Text(" ".to_string()));
            }
            _ => {}
        }
    }

    doc.blocks = normalize_tasks(normalize_callouts(block_stack.pop().unwrap_or_default()));
    doc
}

fn normalize_tasks(blocks: Vec<Block>) -> Vec<Block> {
    blocks
        .into_iter()
        .map(|block| match block {
            Block::List {
                ordered,
                start,
                items,
            } => Block::List {
                ordered,
                start,
                items: items
                    .into_iter()
                    .map(|item| {
                        let mut item = normalize_tasks(item);
                        if let Some(Block::Paragraph(inlines)) = item.first() {
                            if let Some((checked, content)) = task_item(inlines) {
                                item[0] = Block::TaskItem { checked, content };
                            }
                        }
                        item
                    })
                    .collect(),
            },
            Block::BlockQuote(content) => Block::BlockQuote(normalize_tasks(content)),
            Block::Callout {
                kind,
                title,
                content,
            } => Block::Callout {
                kind,
                title,
                content: normalize_tasks(content),
            },
            Block::FootnoteDefinition { name, content } => Block::FootnoteDefinition {
                name,
                content: normalize_tasks(content),
            },
            other => other,
        })
        .collect()
}

fn task_item(inlines: &[Inline]) -> Option<(bool, Vec<Inline>)> {
    let Inline::Text(marker) = inlines.first()? else {
        return None;
    };
    let checked = match marker.as_str() {
        "[ ] " => false,
        "[x] " | "[X] " => true,
        _ => return None,
    };
    Some((checked, inlines[1..].to_vec()))
}

/// Converts block quotes whose first line is an Obsidian callout header into a
/// semantic callout block. Keeping this as a post-parse transformation lets
/// pulldown-cmark retain its normal block quote and nested-block semantics.
fn normalize_callouts(blocks: Vec<Block>) -> Vec<Block> {
    blocks
        .into_iter()
        .map(|block| match block {
            Block::BlockQuote(mut content) => {
                content = normalize_callouts(content);
                let Some(Block::Paragraph(header)) = content.first() else {
                    return Block::BlockQuote(content);
                };
                let Some((kind, title, remainder)) = parse_callout_header(header) else {
                    return Block::BlockQuote(content);
                };
                content.remove(0);
                if !remainder.is_empty() {
                    content.insert(0, Block::Paragraph(remainder));
                }
                Block::Callout {
                    kind,
                    title,
                    content,
                }
            }
            Block::List {
                ordered,
                start,
                items,
            } => Block::List {
                ordered,
                start,
                items: items.into_iter().map(normalize_callouts).collect(),
            },
            Block::Callout {
                kind,
                title,
                content,
            } => Block::Callout {
                kind,
                title,
                content: normalize_callouts(content),
            },
            Block::FootnoteDefinition { name, content } => Block::FootnoteDefinition {
                name,
                content: normalize_callouts(content),
            },
            other => other,
        })
        .collect()
}

/// Parses the first, textual inline of an Obsidian callout header.
///
/// Folding markers (`+` and `-`) are intentionally accepted and discarded for
/// now: the terminal reader renders all callout contents expanded rather than
/// exposing an inaccessible collapsed state.
fn parse_callout_header(inlines: &[Inline]) -> Option<(String, Option<String>, Vec<Inline>)> {
    // With `ENABLE_WIKILINKS`, pulldown-cmark can split `[!NOTE]` into
    // separate text events (`[`, `!NOTE`, `]`). Reassemble only this leading
    // sequence; no formatting is accepted inside a callout marker.
    let mut marker = String::new();
    let mut marker_end = None;
    for (index, inline) in inlines.iter().enumerate() {
        let Inline::Text(text) = inline else {
            return None;
        };
        marker.push_str(text);
        if let Some(close) = marker.find(']') {
            marker_end = Some((index, close));
            break;
        }
        if marker.len() > 128 {
            return None;
        }
    }
    let (marker_index, close) = marker_end?;
    let rest = marker.strip_prefix("[!")?;
    let kind = &rest[..close - 2];
    if kind.is_empty()
        || !kind.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return None;
    }
    let marker_tail = &marker[close + 1..];
    let (title, remainder_start) = if !marker_tail.is_empty() {
        (callout_title(marker_tail), marker_index + 1)
    } else if let Some(Inline::Text(next)) = inlines.get(marker_index + 1) {
        // A standalone space is the soft break inserted between a header-only
        // marker and its body. A non-empty chunk is the optional header title.
        if next == " " {
            (None, marker_index + 2)
        } else {
            (callout_title(next), marker_index + 2)
        }
    } else {
        (None, marker_index + 1)
    };
    Some((
        kind.to_ascii_uppercase(),
        title,
        inlines[remainder_start..].to_vec(),
    ))
}

fn callout_title(value: &str) -> Option<String> {
    let value = value.trim();
    let value = value
        .strip_prefix('+')
        .or_else(|| value.strip_prefix('-'))
        .unwrap_or(value);
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// Sentinel characters that carry an Obsidian `==highlight==` span through
/// pulldown-cmark. Private-use code points are used so ordinary text cannot
/// accidentally produce them.
const HIGHLIGHT_START: char = '\u{E000}';
const HIGHLIGHT_END: char = '\u{E001}';

fn normalize_math_delimiters(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::with_capacity(text.len());
    let mut i = 0;
    let mut at_line_start = true;
    let mut in_fence = false;
    let mut fence_char = '`';
    let mut fence_len = 0;
    let mut inline_code_ticks = 0usize;
    let mut in_highlight = false;

    while i < chars.len() {
        let ch = chars[i];

        // ── fenced code: copy verbatim until the matching closing fence ──────
        if in_fence {
            if at_line_start && ch == fence_char {
                let mut run = 0;
                while i + run < chars.len() && chars[i + run] == fence_char {
                    run += 1;
                }
                if run >= fence_len {
                    in_fence = false;
                    for offset in 0..run {
                        output.push(chars[i + offset]);
                    }
                    i += run;
                    at_line_start = false;
                    continue;
                }
            }
            output.push(ch);
            at_line_start = ch == '\n';
            i += 1;
            continue;
        }

        // ── opening fence at line start ──────────────────────────────────────
        if at_line_start && (ch == '`' || ch == '~') {
            let mut run = 0;
            while i + run < chars.len() && chars[i + run] == ch {
                run += 1;
            }
            if run >= 3 {
                in_fence = true;
                fence_char = ch;
                fence_len = run;
            } else if ch == '`' {
                // A short backtick run at line start still opens/closes an
                // inline code span; only three or more start a fence.
                if inline_code_ticks == 0 {
                    inline_code_ticks = run;
                } else if run == inline_code_ticks {
                    inline_code_ticks = 0;
                }
            }
            for offset in 0..run {
                output.push(chars[i + offset]);
            }
            i += run;
            at_line_start = false;
            continue;
        }

        // ── inline code spans ────────────────────────────────────────────────
        if ch == '`' {
            let mut run = 0;
            while i + run < chars.len() && chars[i + run] == '`' {
                run += 1;
            }
            if inline_code_ticks == 0 {
                inline_code_ticks = run;
            } else if run == inline_code_ticks {
                inline_code_ticks = 0;
            }
            for offset in 0..run {
                output.push(chars[i + offset]);
            }
            i += run;
            at_line_start = false;
            continue;
        }

        // ── Obsidian comments: `%%…%%` disappears from rendered output ───────
        if inline_code_ticks == 0 && ch == '%' && chars.get(i + 1) == Some(&'%') {
            i += 2;
            while i < chars.len() {
                if chars[i] == '%' && chars.get(i + 1) == Some(&'%') {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }

        // ── math delimiter normalization outside code ────────────────────────
        if inline_code_ticks == 0 && ch == '\\' {
            match chars.get(i + 1).copied() {
                Some('(') | Some(')') => {
                    output.push('$');
                    i += 2;
                    at_line_start = false;
                    continue;
                }
                Some('[') | Some(']') => {
                    output.push_str("$$");
                    i += 2;
                    at_line_start = false;
                    continue;
                }
                _ => {}
            }
        }

        // ── `==highlight==` (not a setext underline, same line only) ─────────
        if inline_code_ticks == 0 && ch == '=' && chars.get(i + 1) == Some(&'=') {
            if in_highlight {
                output.push(HIGHLIGHT_END);
                in_highlight = false;
                i += 2;
                at_line_start = false;
                continue;
            }
            let is_setext = at_line_start && is_setext_underline(&chars, i);
            if !is_setext && has_closing_highlight(&chars, i) {
                output.push(HIGHLIGHT_START);
                in_highlight = true;
                i += 2;
                at_line_start = false;
                continue;
            }
        }

        if ch == '\n' && in_highlight {
            // A highlight never spans a line; close it defensively.
            output.push(HIGHLIGHT_END);
            in_highlight = false;
        }
        at_line_start = ch == '\n';
        output.push(ch);
        i += 1;
    }

    if in_highlight {
        output.push(HIGHLIGHT_END);
    }
    wrap_standalone_latex_math_envs(&output)
}

/// Wraps standalone `\begin{equation}...\end{equation}` / `align` / `gather` / `multline`
/// blocks in `$$...$$` when they appear at line start outside fenced code or `$$` blocks,
/// so native LaTeX equations in `.md` files render directly as 2D Unicode math.
fn wrap_standalone_latex_math_envs(text: &str) -> String {
    const MATH_ENVS: &[&str] = &[
        "equation",
        "equation*",
        "align",
        "align*",
        "gather",
        "gather*",
        "multline",
        "multline*",
        "displaymath",
    ];

    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::with_capacity(text.len() + 32);
    let mut in_fence = false;
    let mut in_dollar_math = false;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
        }
        if !in_fence && trimmed.starts_with("$$") {
            let count = line.matches("$$").count();
            if count % 2 == 1 {
                in_dollar_math = !in_dollar_math;
            }
        }

        if !in_fence && !in_dollar_math {
            if let Some(env) = MATH_ENVS
                .iter()
                .copied()
                .find(|e| trimmed.starts_with(&format!("\\begin{{{e}}}")))
            {
                let end_marker = format!("\\end{{{env}}}");
                out.push_str("$$\n");
                let is_align = env.starts_with("align");
                if is_align {
                    out.push_str("\\begin{aligned}\n");
                }
                if let Some(after_begin) = trimmed.strip_prefix(&format!("\\begin{{{env}}}")) {
                    if !after_begin.trim().is_empty() {
                        out.push_str(after_begin.trim());
                        out.push('\n');
                    }
                }
                i += 1;
                while i < lines.len() {
                    let l = lines[i].trim();
                    if let Some(before_end) = l.strip_suffix(&end_marker) {
                        if !before_end.trim().is_empty() {
                            out.push_str(before_end.trim());
                            out.push('\n');
                        }
                        i += 1;
                        break;
                    }
                    if l.starts_with(&end_marker) {
                        i += 1;
                        break;
                    }
                    out.push_str(lines[i]);
                    out.push('\n');
                    i += 1;
                }
                if is_align {
                    out.push_str("\\end{aligned}\n");
                }
                out.push_str("$$\n");
                continue;
            }
        }

        out.push_str(line);
        if i + 1 < lines.len() || text.ends_with('\n') {
            out.push('\n');
        }
        i += 1;
    }

    out
}

/// True when the `=` run beginning at `start` is the entire line, i.e. a
/// setext heading underline rather than an inline highlight.
fn is_setext_underline(chars: &[char], start: usize) -> bool {
    let mut i = start;
    while i < chars.len() && chars[i] == '=' {
        i += 1;
    }
    while i < chars.len() && chars[i] != '\n' {
        if chars[i] != ' ' && chars[i] != '\t' {
            return false;
        }
        i += 1;
    }
    true
}

/// True when a non-empty `==…==` span closes later on the same line.
fn has_closing_highlight(chars: &[char], start: usize) -> bool {
    let mut i = start + 2;
    if i >= chars.len() || chars[i] == '=' || chars[i] == '\n' {
        return false;
    }
    while i + 1 < chars.len() {
        if chars[i] == '\n' {
            return false;
        }
        if chars[i] == '=' && chars[i + 1] == '=' {
            return true;
        }
        i += 1;
    }
    false
}

/// Splits a text chunk on highlight sentinels, then parses wiki links inside
/// each segment. Highlight content without nested formatting is the common
/// Obsidian case; sentinel pairs that span pulldown events degrade to plain
/// text instead of leaking private-use characters.
fn parse_inline_extensions(text: &str, inlines: &mut Vec<Inline>) {
    let mut rest = text;
    let mut highlighted = false;
    loop {
        let marker = if highlighted {
            HIGHLIGHT_END
        } else {
            HIGHLIGHT_START
        };
        match rest.find(marker) {
            Some(position) => {
                push_inline_segment(&rest[..position], inlines, highlighted);
                highlighted = !highlighted;
                rest = &rest[position + marker.len_utf8()..];
            }
            None => {
                push_inline_segment(rest, inlines, highlighted);
                break;
            }
        }
    }
}

fn push_inline_segment(segment: &str, inlines: &mut Vec<Inline>, highlighted: bool) {
    if segment.is_empty() {
        return;
    }
    if highlighted {
        let mut inner = Vec::new();
        parse_wiki_links_in_text(segment, &mut inner);
        inlines.push(Inline::Highlight(inner));
    } else {
        parse_wiki_links_in_text(segment, inlines);
    }
}

/// Parses inline `[[WikiLink]]` occurrences from raw text chunks.
fn parse_wiki_links_in_text(text: &str, inlines: &mut Vec<Inline>) {
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find("[[") {
        let abs_start = cursor + start;
        if abs_start > cursor {
            push_text_with_tags(&text[cursor..abs_start], inlines);
        }
        if let Some(end) = text[abs_start + 2..].find("]]") {
            let abs_end = abs_start + 2 + end;
            let inner = &text[abs_start + 2..abs_end];
            if let Some((target, label)) = inner.split_once('|') {
                inlines.push(Inline::WikiLink {
                    target: target.trim().to_string(),
                    label: Some(label.trim().to_string()),
                });
            } else {
                inlines.push(Inline::WikiLink {
                    target: inner.trim().to_string(),
                    label: None,
                });
            }
            cursor = abs_end + 2;
        } else {
            push_text_with_tags(&text[abs_start..], inlines);
            cursor = text.len();
            break;
        }
    }
    if cursor < text.len() {
        push_text_with_tags(&text[cursor..], inlines);
    }
}

/// Splits a plain-text run into text and Obsidian `#tag` pieces.
///
/// Tags may contain letters, digits, `_`, `-` and `/` (nested tags). A `#`
/// glued to a preceding word character is not a tag, which keeps fragments
/// such as `C#` and URL anchors like `/docs#setup` as plain text.
fn push_text_with_tags(text: &str, inlines: &mut Vec<Inline>) {
    let mut text_start = 0;
    let mut scan = 0;
    while let Some(relative) = text[scan..].find('#') {
        let start = scan + relative;
        let previous = text[..start].chars().next_back();
        if previous.is_some_and(is_tag_word_char) {
            scan = start + 1;
            continue;
        }

        let mut end = start + 1;
        for (index, character) in text[end..].char_indices() {
            let first = index == 0;
            let valid = if first {
                character.is_alphabetic() || character == '_'
            } else {
                character.is_alphanumeric()
                    || character == '_'
                    || character == '-'
                    || character == '/'
            };
            if !valid {
                break;
            }
            end += character.len_utf8();
        }

        let tag = text[start + 1..end].trim_end_matches(['-', '/']);
        if tag.is_empty() {
            scan = start + 1;
            continue;
        }

        if start > text_start {
            inlines.push(Inline::Text(text[text_start..start].to_string()));
        }
        inlines.push(Inline::Tag(tag.to_string()));
        text_start = start + 1 + tag.len();
        scan = text_start;
    }
    if text_start < text.len() {
        inlines.push(Inline::Text(text[text_start..].to_string()));
    }
}

fn is_tag_word_char(character: char) -> bool {
    character.is_alphanumeric()
        || character == '_'
        || character == '-'
        || character == '/'
        || character == '#'
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Block, Inline};

    #[test]
    fn test_markdown_link_target_preservation() {
        let md = "[docs](https://example.org)";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        if let Block::Paragraph(inlines) = &doc.blocks[0] {
            assert_eq!(inlines.len(), 1);
            match &inlines[0] {
                Inline::Link { target, text } => {
                    assert_eq!(target, "https://example.org");
                    assert_eq!(text[0].plain_text(), "docs");
                }
                _ => panic!("Expected Inline::Link"),
            }
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn test_image_alt_and_url_preservation() {
        let md = "![Heap diagram](images/heap.png)";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Image { alt, url } => {
                assert_eq!(alt, "Heap diagram");
                assert_eq!(url, "images/heap.png");
            }
            _ => panic!("Expected Block::Image"),
        }
    }

    #[test]
    fn normalizes_obsidian_math_delimiters_but_not_code() {
        let doc = parse_markdown("Inline \\(x^2\\) and \\[\\frac{a}{b}\\].\n\n`\\(code\\)`\n");
        assert!(
            matches!(
                &doc.blocks[1],
                Block::Paragraph(inlines) if inlines.iter().any(|inline| matches!(inline, Inline::Math(value) if value == "x^2"))
            ),
            "{doc:?}"
        );
        assert!(matches!(
            &doc.blocks[0],
            Block::Math(value) if value == "\\frac{a}{b}"
        ));
        assert!(matches!(
            &doc.blocks[2],
            Block::Paragraph(inlines) if inlines.iter().any(|inline| matches!(inline, Inline::Code(_)))
        ));
    }

    #[test]
    fn parses_obsidian_callouts_and_preserves_their_content() {
        let doc = parse_markdown(
            "> [!warning]- Careful\n> This remains readable in the terminal.\n>\n> - nested detail\n",
        );
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Callout {
                kind,
                title,
                content,
            } => {
                assert_eq!(kind, "WARNING");
                assert_eq!(title.as_deref(), Some("Careful"));
                assert!(content.iter().any(|block| matches!(
                    block,
                    Block::Paragraph(inlines) if inlines.iter().any(|inline| inline.plain_text().contains("This remains readable"))
                )));
                assert!(content
                    .iter()
                    .any(|block| matches!(block, Block::List { .. })));
            }
            other => panic!("expected callout, got {other:?}"),
        }
    }

    #[test]
    fn does_not_turn_an_ordinary_quote_into_a_callout() {
        let doc = parse_markdown("> [normal text] is still a quote\n");
        assert!(matches!(doc.blocks.as_slice(), [Block::BlockQuote(_)]));
    }

    #[test]
    fn keeps_footnote_definitions_as_readable_blocks() {
        let doc = parse_markdown("A reference[^note].\n\n[^note]: Detail with **formatting**.\n");
        assert!(matches!(
            doc.blocks.last(),
            Some(Block::FootnoteDefinition { name, content })
                if name == "note" && !content.is_empty()
        ));
    }

    #[test]
    fn parses_obsidian_highlights_and_hides_comments() {
        let doc = parse_markdown("A ==marked== word. %%hidden%% After.\n");
        match &doc.blocks[0] {
            Block::Paragraph(inlines) => {
                let text = inlines
                    .iter()
                    .map(|inline| inline.plain_text())
                    .collect::<String>();
                assert_eq!(text, "A marked word.  After.");
                assert!(inlines.iter().any(|inline| matches!(
                    inline,
                    Inline::Highlight(inner)
                        if inner.iter().any(|item| item.plain_text() == "marked")
                )));
                assert!(!text.contains("hidden"));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_obsidian_tags_and_ignores_word_fragments() {
        let doc = parse_markdown("See #rust and #project/web, C# and /docs#setup.\n");
        match &doc.blocks[0] {
            Block::Paragraph(inlines) => {
                let tags: Vec<&str> = inlines
                    .iter()
                    .filter_map(|inline| match inline {
                        Inline::Tag(tag) => Some(tag.as_str()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(tags, ["rust", "project/web"]);
                let text = inlines
                    .iter()
                    .map(|inline| inline.plain_text())
                    .collect::<String>();
                assert!(text.contains("C# and /docs#setup."), "{text}");
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
    }

    #[test]
    fn tags_are_not_parsed_inside_code() {
        let doc = parse_markdown("`#code` and ``#also-code``\n");
        let rendered = format!("{doc:?}");
        assert!(!rendered.contains("Inline::Tag"), "{rendered}");
    }

    #[test]
    fn comments_and_highlights_do_not_touch_code() {
        let doc = parse_markdown("`%%keep%% ==raw==`\n\n```\n==fenced== %%fenced%%\n```\n");
        let rendered = format!("{doc:?}");
        assert!(rendered.contains("%%keep%%"), "{rendered}");
        assert!(rendered.contains("==raw=="));
        assert!(rendered.contains("==fenced=="));
        assert!(rendered.contains("%%fenced%%"));
    }

    #[test]
    fn setext_underline_is_not_a_highlight() {
        let doc = parse_markdown("Title\n=====\n");
        assert!(matches!(doc.blocks.as_slice(), [Block::Heading(1, _)]));
    }

    #[test]
    fn parses_obsidian_embed_as_an_image_event_for_rendering() {
        let doc = parse_markdown("![[diagram.png]]");
        assert!(matches!(
            doc.blocks.as_slice(),
            [Block::Image { alt, url }] if alt == "diagram.png" && url == "diagram.png"
        ));
    }

    #[test]
    fn test_all_link_types_and_no_collision() {
        let md = r#"
[docs](https://example.org)
[relative](../foo.md)
[anchor](#heading)
![image](assets/test.png)
[[Heap]]
[[Heap|Heap Exploitation]]
[[security::Heap]]
"#;
        let doc = parse_markdown(md);
        assert!(doc.blocks.len() >= 2);
    }

    #[test]
    fn test_heading_and_paragraph() {
        let md = "# Memory Management\n\nExploiting the glibc heap.";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Heading(1, inlines) => {
                assert_eq!(inlines[0], Inline::Text("Memory Management".into()));
            }
            _ => panic!("Expected Heading(1, ..)"),
        }
        match &doc.blocks[1] {
            Block::Paragraph(inlines) => {
                assert_eq!(
                    inlines[0],
                    Inline::Text("Exploiting the glibc heap.".into())
                );
            }
            _ => panic!("Expected Paragraph"),
        }
    }

    #[test]
    fn test_bold_italic_and_nested() {
        let md = "Normal **bold text** and *italic text* and ***bold italic***.";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        if let Block::Paragraph(inlines) = &doc.blocks[0] {
            assert!(inlines.iter().any(|i| matches!(i, Inline::Bold(_))));
            assert!(inlines.iter().any(|i| matches!(i, Inline::Italic(_))));
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn test_wiki_links_and_aliases() {
        let md = "See [[Heap]] and [[Heap|Heap Exploitation]] for details.";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        if let Block::Paragraph(inlines) = &doc.blocks[0] {
            assert_eq!(inlines[0], Inline::Text("See ".into()));
            assert_eq!(
                inlines[1],
                Inline::WikiLink {
                    target: "Heap".into(),
                    label: None,
                }
            );
            assert_eq!(inlines[2], Inline::Text(" and ".into()));
            assert_eq!(
                inlines[3],
                Inline::WikiLink {
                    target: "Heap".into(),
                    label: Some("Heap Exploitation".into()),
                }
            );
            assert_eq!(inlines[4], Inline::Text(" for details.".into()));
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn test_lists_and_tasks() {
        let md = "3. [ ] Task 1\n4. [x] Task 2\n   - Nested item\n5. Item 3";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::List {
                ordered,
                start,
                items,
            } => {
                assert!(*ordered);
                assert_eq!(*start, 3);
                assert!(
                    matches!(
                        items[0].as_slice(),
                        [Block::TaskItem { checked: false, .. }]
                    ),
                    "{doc:?}"
                );
                assert!(
                    matches!(
                        items[1].as_slice(),
                        [Block::TaskItem { checked: true, .. }, Block::List { .. }]
                    ),
                    "{doc:?}"
                );
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn test_code_blocks() {
        let md = "```rust\nfn main() {\n    println!(\"hi\");\n}\n```";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::CodeBlock { language, content } => {
                assert_eq!(language.as_deref(), Some("rust"));
                assert!(content.contains("println!(\"hi\");"));
            }
            _ => panic!("Expected CodeBlock"),
        }
    }

    #[test]
    fn test_tables() {
        let md = "| Header A | Header B |\n|---|---|\n| Cell 1 | Cell 2 |";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Table { headers, rows } => {
                assert_eq!(headers.len(), 2);
                assert_eq!(rows.len(), 1);
            }
            _ => panic!("Expected Table"),
        }
    }
}
