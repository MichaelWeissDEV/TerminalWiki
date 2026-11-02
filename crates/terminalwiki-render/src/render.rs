//! Rendering semantic Document AST into terminal Spans and RenderedLines (spec §55, §60).

use std::collections::HashMap;
use std::path::Path;

use terminalwiki_core::link::heading_anchor;
use terminalwiki_core::unicode::display_width;

use crate::ansi::{ColorMode, Style};
use crate::diagram::{is_diagram_language, render_diagram_block};
use crate::document::{
    Block, Document, Inline, LinkTarget, RenderedDocument, RenderedHeading, RenderedLink, Span,
};
use crate::highlight::highlight;
use crate::image_view::render_inline_image;
use crate::math::layout_math;
use crate::theme::{SemanticColor, Theme};

pub struct Renderer<'a> {
    max_content_width: usize,
    theme: &'a Theme,
    color_mode: ColorMode,
    base_dir: Option<&'a Path>,
}

impl<'a> Renderer<'a> {
    pub fn new(max_content_width: usize, theme: &'a Theme, color_mode: ColorMode) -> Self {
        Self {
            max_content_width: max_content_width.max(40),
            theme,
            color_mode,
            base_dir: None,
        }
    }

    pub fn with_base_dir(mut self, base_dir: Option<&'a Path>) -> Self {
        self.base_dir = base_dir;
        self
    }

    pub fn render_document(&self, doc: &Document) -> RenderedDocument {
        let mut rdoc = RenderedDocument::default();
        let mut heading_anchors = HashMap::new();

        for (idx, block) in doc.blocks.iter().enumerate() {
            self.render_block(block, &mut rdoc, 0, &mut heading_anchors);
            if idx + 1 < doc.blocks.len() {
                rdoc.lines.push(Vec::new()); // blank line separation between top-level blocks
            }
        }

        rdoc
    }

    fn render_block(
        &self,
        block: &Block,
        rdoc: &mut RenderedDocument,
        indent_level: usize,
        heading_anchors: &mut HashMap<String, usize>,
    ) {
        let indent = "  ".repeat(indent_level);

        match block {
            Block::Heading(level, inlines) => {
                let text = inlines.iter().map(|i| i.plain_text()).collect::<String>();
                let current_line = rdoc.lines.len();
                let base_anchor = heading_anchor(&text);
                let base_anchor = if base_anchor.is_empty() {
                    "section".to_string()
                } else {
                    base_anchor
                };
                let occurrence = heading_anchors.entry(base_anchor.clone()).or_insert(0);
                let anchor = if *occurrence == 0 {
                    base_anchor
                } else {
                    format!("{base_anchor}-{occurrence}")
                };
                *occurrence += 1;
                rdoc.headings.push(RenderedHeading {
                    level: *level,
                    text: text.clone(),
                    anchor,
                    line: current_line,
                });

                let prefix = "#".repeat(*level);
                let heading_style = self.theme.style(SemanticColor::Heading).bold();
                rdoc.lines.push(vec![
                    Span {
                        text: format!("{indent}{prefix} "),
                        style: self.theme.style(SemanticColor::Muted),
                    },
                    Span {
                        text,
                        style: heading_style,
                    },
                ]);
            }

            Block::Paragraph(inlines) => {
                let (spans, marks) = self.inline_runs(inlines);
                let width = self
                    .max_content_width
                    .saturating_sub(indent_level * 2)
                    .max(1);
                let wrapped = wrap_styled(&spans, width);
                let base_line = rdoc.lines.len();
                let indent_width = display_width(&indent);

                for wrapped_line in &wrapped {
                    let mut full_line = Vec::new();
                    if !indent.is_empty() {
                        full_line.push(Span {
                            text: indent.clone(),
                            style: Style::new(),
                        });
                    }
                    full_line.extend(wrapped_line.spans.iter().cloned());
                    rdoc.lines.push(full_line);
                }

                for mark in marks {
                    let mut emitted = false;
                    for (index, wrapped_line) in wrapped.iter().enumerate() {
                        let start = mark.start.max(wrapped_line.start);
                        let end = mark.end.min(wrapped_line.end);
                        if start >= end {
                            continue;
                        }
                        emitted = true;
                        rdoc.links.push(RenderedLink {
                            line: base_line + index,
                            start_column: indent_width + (start - wrapped_line.start),
                            end_column: indent_width + (end - wrapped_line.start),
                            target: mark.target.clone(),
                            label: mark.label.clone(),
                        });
                    }
                    if !emitted {
                        // Zero-width or fully elided link: keep it attached to
                        // the first line so keyboard cycling still reaches it.
                        rdoc.links.push(RenderedLink {
                            line: base_line,
                            start_column: indent_width,
                            end_column: indent_width,
                            target: mark.target,
                            label: mark.label,
                        });
                    }
                }
            }

            Block::CodeBlock { language, content } => {
                if let Some(lang) = language.as_deref() {
                    if is_diagram_language(Some(lang)) {
                        if let Some(diag_lines) = render_diagram_block(
                            lang,
                            content,
                            &indent,
                            self.max_content_width,
                            self.theme,
                        ) {
                            rdoc.lines.extend(diag_lines);
                            return;
                        }
                    }
                }

                // Header bar
                let lang_label = language.as_deref().unwrap_or("code");
                rdoc.lines.push(vec![
                    Span {
                        text: format!("{indent}```"),
                        style: self.theme.style(SemanticColor::Muted),
                    },
                    Span {
                        text: lang_label.to_string(),
                        style: self.theme.style(SemanticColor::Accent),
                    },
                ]);

                let code_lines = highlight(content, language.as_deref(), self.theme);
                for line in code_lines {
                    let mut row = vec![Span {
                        text: format!("{indent}  "),
                        style: Style::new(),
                    }];
                    row.extend(line);
                    rdoc.lines.push(row);
                }

                rdoc.lines.push(vec![Span {
                    text: format!("{indent}```"),
                    style: self.theme.style(SemanticColor::Muted),
                }]);
            }

            Block::BlockQuote(blocks) => {
                let prefix = Span {
                    text: format!("{indent}│ "),
                    style: self.theme.style(SemanticColor::Muted),
                };
                for b in blocks {
                    let mut sub_rdoc = RenderedDocument::default();
                    self.render_block(b, &mut sub_rdoc, 0, heading_anchors);
                    self.append_sub(rdoc, sub_rdoc, &prefix, &prefix, true);
                }
            }

            Block::Callout {
                kind,
                title,
                content,
            } => {
                let label = title.as_deref().unwrap_or(kind.as_str());
                let callout_style = self.theme.style(SemanticColor::Accent).bold();
                rdoc.lines.push(vec![
                    Span {
                        text: format!("{indent}┌─ [!] "),
                        style: callout_style.clone(),
                    },
                    Span {
                        text: label.to_string(),
                        style: callout_style,
                    },
                ]);

                let prefix = Span {
                    text: format!("{indent}│  "),
                    style: self.theme.style(SemanticColor::Muted),
                };
                for b in content {
                    let mut sub_rdoc = RenderedDocument::default();
                    self.render_block(b, &mut sub_rdoc, 0, heading_anchors);
                    self.append_sub(rdoc, sub_rdoc, &prefix, &prefix, true);
                }
                rdoc.lines.push(vec![Span {
                    text: format!("{indent}└──"),
                    style: self.theme.style(SemanticColor::Muted),
                }]);
            }

            Block::FootnoteDefinition { name, content } => {
                let footnote_style = self.theme.style(SemanticColor::Muted);
                rdoc.lines.push(vec![Span {
                    text: format!("{indent}[^{}]", name),
                    style: footnote_style.clone(),
                }]);
                let prefix = Span {
                    text: format!("{indent}  "),
                    style: footnote_style.clone(),
                };
                for block in content {
                    let mut sub_rdoc = RenderedDocument::default();
                    self.render_block(block, &mut sub_rdoc, 0, heading_anchors);
                    self.append_sub(rdoc, sub_rdoc, &prefix, &prefix, true);
                }
            }

            Block::List {
                ordered,
                start,
                items,
            } => {
                for (i, item) in items.iter().enumerate() {
                    let mut marker = if *ordered {
                        let num = start + i as u64;
                        format!("{indent}{num}. ")
                    } else {
                        format!("{indent}• ")
                    };
                    let marker_style = self.theme.style(SemanticColor::Accent);

                    // Obsidian task items replace the bullet with a checkbox
                    // glyph rather than showing a bullet and a `[ ]` marker.
                    let mut body: &[Block] = item;
                    let mut task: Option<(Vec<Span>, Vec<LinkMark>)> = None;
                    if let Some(Block::TaskItem { checked, content }) = item.first() {
                        let glyph = if *checked { "☑" } else { "☐" };
                        marker = format!("{indent}{glyph} ");
                        task = Some(self.inline_runs(content));
                        body = &item[1..];
                    }

                    let first_prefix = Span {
                        text: marker.clone(),
                        style: marker_style,
                    };
                    let rest_prefix = Span {
                        text: " ".repeat(display_width(&marker)),
                        style: Style::new(),
                    };
                    let mut emitted_marker = false;

                    if let Some((spans, marks)) = task {
                        let line = rdoc.lines.len();
                        let mut row = vec![first_prefix.clone()];
                        row.extend(spans);
                        rdoc.lines.push(row);
                        emitted_marker = true;
                        self.register_marks_on_line(
                            rdoc,
                            line,
                            display_width(&first_prefix.text),
                            &marks,
                        );
                    }

                    for b in body {
                        let mut sub_rdoc = RenderedDocument::default();
                        self.render_block(b, &mut sub_rdoc, 0, heading_anchors);
                        let first = !emitted_marker;
                        emitted_marker = true;
                        self.append_sub(rdoc, sub_rdoc, &first_prefix, &rest_prefix, first);
                    }
                }
            }

            Block::TaskItem { checked, content } => {
                let marker = if *checked { "☑ " } else { "☐ " };
                let marker = format!("{indent}{marker}");
                let (spans, marks) = self.inline_runs(content);
                let line = rdoc.lines.len();
                let mut row = vec![Span {
                    text: marker.clone(),
                    style: self.theme.style(SemanticColor::Accent),
                }];
                row.extend(spans);
                rdoc.lines.push(row);
                self.register_marks_on_line(rdoc, line, display_width(&marker), &marks);
            }

            Block::Table { headers, rows } => {
                self.render_table(headers, rows, rdoc, indent_level);
            }

            Block::HorizontalRule => {
                let rule_width = self.max_content_width.min(60);
                rdoc.lines.push(vec![Span {
                    text: format!("{indent}{}", "─".repeat(rule_width)),
                    style: self.theme.style(SemanticColor::Muted),
                }]);
            }

            Block::Image { alt, url } => {
                if is_image_asset(url) {
                    let img_lines = render_inline_image(
                        self.base_dir,
                        alt,
                        url,
                        &indent,
                        self.max_content_width,
                        self.theme,
                        self.color_mode,
                    );
                    rdoc.lines.extend(img_lines);
                } else {
                    // pulldown-cmark maps Obsidian `![[Note#Heading]]` to an
                    // image event. A non-image destination is therefore a
                    // safe, navigable text fallback rather than a misleading
                    // broken image placeholder.
                    let (display_text, link_target) = wiki_link_target(url, Some(alt.clone()));
                    let display = format!("{indent}↳ [[{display_text}]]");
                    let line = rdoc.lines.len();
                    let start_column = display_width(&indent) + display_width("↳ ");
                    rdoc.links.push(RenderedLink {
                        line,
                        start_column,
                        end_column: start_column + display_width(&format!("[[{display_text}]]")),
                        target: link_target,
                        label: display_text,
                    });
                    rdoc.lines.push(vec![Span {
                        text: display,
                        style: self.theme.style(SemanticColor::Link).bold(),
                    }]);
                }
            }

            Block::Math(formula) => {
                let layout = layout_math(formula);
                for line in layout.lines {
                    rdoc.lines.push(vec![Span {
                        text: format!("{indent}  {line}"),
                        style: self.theme.style(SemanticColor::Code),
                    }]);
                }
            }
        }
    }

    /// Renders an inline run into styled spans plus link marks. Marks carry
    /// absolute cell offsets in the unwrapped run, which lets wrapped
    /// paragraphs map hit regions onto the correct visual line.
    fn inline_runs(&self, inlines: &[Inline]) -> (Vec<Span>, Vec<LinkMark>) {
        let mut spans = Vec::new();
        let mut marks = Vec::new();
        self.collect_inlines(inlines, &mut spans, &mut marks);
        (spans, marks)
    }

    fn collect_inlines(
        &self,
        inlines: &[Inline],
        spans: &mut Vec<Span>,
        marks: &mut Vec<LinkMark>,
    ) {
        for inline in inlines {
            let start_column = spans
                .iter()
                .map(|span| display_width(&span.text))
                .sum::<usize>();
            match inline {
                Inline::Text(t) => {
                    spans.push(Span {
                        text: t.clone(),
                        style: self.theme.style(SemanticColor::Foreground),
                    });
                }
                Inline::Bold(inner) => {
                    let from = spans.len();
                    self.collect_inlines(inner, spans, marks);
                    for span in &mut spans[from..] {
                        span.style.bold = true;
                    }
                }
                Inline::Italic(inner) => {
                    let from = spans.len();
                    self.collect_inlines(inner, spans, marks);
                    for span in &mut spans[from..] {
                        span.style.italic = true;
                    }
                }
                Inline::Strike(inner) => {
                    let from = spans.len();
                    self.collect_inlines(inner, spans, marks);
                    for span in &mut spans[from..] {
                        span.style.strikethrough = true;
                    }
                }
                Inline::Highlight(inner) => {
                    let from = spans.len();
                    self.collect_inlines(inner, spans, marks);
                    let highlight = self.theme.style(SemanticColor::Selection).bold();
                    for span in &mut spans[from..] {
                        span.style = highlight.clone();
                    }
                }
                Inline::Code(c) => {
                    spans.push(Span {
                        text: format!("`{c}`"),
                        style: self.theme.style(SemanticColor::Code),
                    });
                }
                Inline::Link { target, text } => {
                    let label = text.iter().map(|i| i.plain_text()).collect::<String>();
                    let link_target = if let Some(anchor) = target.strip_prefix('#') {
                        LinkTarget::Heading(anchor.to_string())
                    } else if target.starts_with("http://")
                        || target.starts_with("https://")
                        || target.starts_with("mailto:")
                    {
                        LinkTarget::External(target.clone())
                    } else {
                        LinkTarget::File {
                            path: std::path::PathBuf::from(target),
                            line_range: None,
                        }
                    };
                    let from = spans.len();
                    self.collect_inlines(text, spans, marks);
                    let end_column = spans
                        .iter()
                        .map(|span| display_width(&span.text))
                        .sum::<usize>();
                    let link_style = self.theme.style(SemanticColor::Link).underline();
                    for span in &mut spans[from..] {
                        span.style = link_style.clone();
                    }
                    marks.push(LinkMark {
                        start: start_column,
                        end: end_column,
                        target: link_target,
                        label,
                    });
                }
                Inline::WikiLink { target, label } => {
                    let (display_text, link_target) = wiki_link_target(target, label.clone());
                    let full_display = format!("[[{display_text}]]");
                    marks.push(LinkMark {
                        start: start_column,
                        end: start_column + display_width(&full_display),
                        target: link_target,
                        label: display_text,
                    });
                    spans.push(Span {
                        text: full_display,
                        style: self.theme.style(SemanticColor::Link).bold(),
                    });
                }
                Inline::Tag(tag) => {
                    let display = format!("#{tag}");
                    marks.push(LinkMark {
                        start: start_column,
                        end: start_column + display_width(&display),
                        target: LinkTarget::Tag(tag.clone()),
                        label: display.clone(),
                    });
                    spans.push(Span {
                        text: display,
                        style: self.theme.style(SemanticColor::Link).italic(),
                    });
                }
                Inline::Footnote(f) => {
                    spans.push(Span {
                        text: f.clone(),
                        style: self.theme.style(SemanticColor::Muted),
                    });
                }
                Inline::Math(m) => {
                    let layout = layout_math(m);
                    spans.push(Span {
                        // Inline contexts cannot occupy more than one terminal
                        // row; preserve the native glyph conversion while using
                        // spaces to flatten a multi-row fraction/root.
                        text: layout.lines.join(" "),
                        style: self.theme.style(SemanticColor::Code),
                    });
                }
            }
        }
    }

    fn register_marks_on_line(
        &self,
        rdoc: &mut RenderedDocument,
        line: usize,
        prefix_width: usize,
        marks: &[LinkMark],
    ) {
        for mark in marks {
            rdoc.links.push(RenderedLink {
                line,
                start_column: prefix_width + mark.start,
                end_column: prefix_width + mark.end,
                target: mark.target.clone(),
                label: mark.label.clone(),
            });
        }
    }

    /// Appends a sub-document with a per-line prefix, shifting its link and
    /// heading metadata into parent coordinates.
    fn append_sub(
        &self,
        rdoc: &mut RenderedDocument,
        sub: RenderedDocument,
        first_prefix: &Span,
        rest_prefix: &Span,
        use_first_prefix: bool,
    ) {
        let base_line = rdoc.lines.len();
        let first_width = display_width(&first_prefix.text);
        let rest_width = display_width(&rest_prefix.text);
        let mut emitted_first = !use_first_prefix;

        for line in sub.lines {
            let prefix = if emitted_first {
                rest_prefix
            } else {
                emitted_first = true;
                first_prefix
            };
            let mut row = vec![prefix.clone()];
            row.extend(line);
            rdoc.lines.push(row);
        }

        for mut link in sub.links {
            link.line += base_line;
            let width = if use_first_prefix && link.line == base_line {
                first_width
            } else {
                rest_width
            };
            link.start_column += width;
            link.end_column += width;
            rdoc.links.push(link);
        }

        for mut heading in sub.headings {
            heading.line += base_line;
            rdoc.headings.push(heading);
        }
    }

    fn render_table(
        &self,
        headers: &[Vec<Inline>],
        rows: &[Vec<Vec<Inline>>],
        rdoc: &mut RenderedDocument,
        indent_level: usize,
    ) {
        let indent = "  ".repeat(indent_level);
        let indent_w = display_width(&indent);
        let num_cols = headers
            .len()
            .max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
        if num_cols == 0 {
            return;
        }

        // Pre-render inline runs for headers and cells so widths and link marks match
        let rendered_headers: Vec<(Vec<Span>, Vec<LinkMark>, usize)> = (0..num_cols)
            .map(|i| {
                if let Some(h) = headers.get(i) {
                    let (mut spans, marks) = self.inline_runs(h);
                    let heading_style = self.theme.style(SemanticColor::Heading).bold();
                    for s in &mut spans {
                        if s.style == self.theme.style(SemanticColor::Foreground) {
                            s.style = heading_style.clone();
                        } else {
                            s.style.bold = true;
                        }
                    }
                    let w: usize = spans.iter().map(|s| display_width(&s.text)).sum();
                    (spans, marks, w)
                } else {
                    (Vec::new(), Vec::new(), 0)
                }
            })
            .collect();

        let rendered_rows: Vec<Vec<(Vec<Span>, Vec<LinkMark>, usize)>> = rows
            .iter()
            .map(|row| {
                (0..num_cols)
                    .map(|i| {
                        if let Some(cell) = row.get(i) {
                            let (spans, marks) = self.inline_runs(cell);
                            let w: usize = spans.iter().map(|s| display_width(&s.text)).sum();
                            (spans, marks, w)
                        } else {
                            (Vec::new(), Vec::new(), 0)
                        }
                    })
                    .collect()
            })
            .collect();

        let mut col_widths = vec![4usize; num_cols];
        for (i, (_, _, w)) in rendered_headers.iter().enumerate() {
            col_widths[i] = col_widths[i].max(*w);
        }
        for row in &rendered_rows {
            for (i, (_, _, w)) in row.iter().enumerate() {
                col_widths[i] = col_widths[i].max(*w);
            }
        }

        let border_style = self.theme.style(SemanticColor::Muted);

        // Top border: ┌───┬───┐
        let top_segments: Vec<String> = col_widths.iter().map(|w| "─".repeat(*w + 2)).collect();
        rdoc.lines.push(vec![
            Span {
                text: indent.clone(),
                style: Style::new(),
            },
            Span {
                text: format!("┌{}┐", top_segments.join("┬")),
                style: border_style.clone(),
            },
        ]);

        // Header row: │ Col1 │ Col2 │
        self.push_table_row(&rendered_headers, &col_widths, &indent, indent_w, rdoc);

        // Middle separator: ├───┼───┤
        let mid_segments: Vec<String> = col_widths.iter().map(|w| "─".repeat(*w + 2)).collect();
        rdoc.lines.push(vec![
            Span {
                text: indent.clone(),
                style: Style::new(),
            },
            Span {
                text: format!("├{}┤", mid_segments.join("┼")),
                style: border_style.clone(),
            },
        ]);

        // Data rows
        for row in &rendered_rows {
            self.push_table_row(row, &col_widths, &indent, indent_w, rdoc);
        }

        // Bottom border: └───┴───┘
        let bot_segments: Vec<String> = col_widths.iter().map(|w| "─".repeat(*w + 2)).collect();
        rdoc.lines.push(vec![
            Span {
                text: indent,
                style: Style::new(),
            },
            Span {
                text: format!("└{}┘", bot_segments.join("┴")),
                style: border_style,
            },
        ]);
    }

    fn push_table_row(
        &self,
        cells: &[(Vec<Span>, Vec<LinkMark>, usize)],
        col_widths: &[usize],
        indent: &str,
        indent_w: usize,
        rdoc: &mut RenderedDocument,
    ) {
        let border_style = self.theme.style(SemanticColor::Muted);
        let line_idx = rdoc.lines.len();
        let mut row_spans = vec![
            Span {
                text: indent.to_string(),
                style: Style::new(),
            },
            Span {
                text: "│".to_string(),
                style: border_style.clone(),
            },
        ];
        let mut col_offset = indent_w + 1;

        for (i, (spans, marks, cell_w)) in cells.iter().enumerate() {
            let target_w = col_widths[i];
            row_spans.push(Span {
                text: " ".to_string(),
                style: Style::new(),
            });
            col_offset += 1;

            self.register_marks_on_line(rdoc, line_idx, col_offset, marks);
            row_spans.extend(spans.iter().cloned());

            let pad = target_w.saturating_sub(*cell_w);
            row_spans.push(Span {
                text: " ".repeat(pad + 1),
                style: Style::new(),
            });
            row_spans.push(Span {
                text: "│".to_string(),
                style: border_style.clone(),
            });
            col_offset += target_w + 2;
        }
        rdoc.lines.push(row_spans);
    }
}

/// A link whose hit region is expressed as absolute cell offsets in an
/// unwrapped inline run.
struct LinkMark {
    start: usize,
    end: usize,
    target: LinkTarget,
    label: String,
}

/// One wrapped visual line plus its cell range in the unwrapped run.
struct WrappedLine {
    spans: Vec<Span>,
    start: usize,
    end: usize,
}

fn char_width(character: char) -> usize {
    unicode_width::UnicodeWidthChar::width(character).unwrap_or(0)
}

/// Word-wraps styled spans while preserving their styles and recording the
/// original cell offsets needed to remap link hit regions.
fn wrap_styled(spans: &[Span], max_width: usize) -> Vec<WrappedLine> {
    let mut chars: Vec<(char, &Style)> = Vec::new();
    for span in spans {
        for character in span.text.chars() {
            chars.push((character, &span.style));
        }
    }

    let mut cell_before = Vec::with_capacity(chars.len() + 1);
    let mut width = 0;
    cell_before.push(0);
    for (character, _) in &chars {
        width += char_width(*character);
        cell_before.push(width);
    }

    let mut result = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        while index < chars.len() && chars[index].0 == ' ' {
            index += 1;
        }
        if index >= chars.len() {
            break;
        }

        let line_start = index;
        let mut line_end = index;
        let mut line_width = 0;

        while line_end < chars.len() {
            let word_start = line_end;
            let mut word_end = word_start;
            let mut word_width = 0;
            while word_end < chars.len() && chars[word_end].0 != ' ' {
                word_width += char_width(chars[word_end].0);
                word_end += 1;
            }
            if word_start == word_end {
                break;
            }

            let needed = if line_width == 0 {
                word_width
            } else {
                line_width + 1 + word_width
            };

            if line_width == 0 && word_width > max_width {
                // Hard-break a single overlong word.
                let mut taken = 0;
                let mut cursor = word_start;
                while cursor < word_end {
                    let cell = char_width(chars[cursor].0);
                    if taken + cell > max_width {
                        break;
                    }
                    taken += cell;
                    cursor += 1;
                }
                line_end = cursor;
                break;
            }

            if line_width > 0 && needed > max_width {
                break;
            }

            line_width = needed;
            line_end = word_end;
            if line_end < chars.len() && chars[line_end].0 == ' ' {
                line_end += 1;
            }
        }

        let mut end = line_end;
        while end > line_start && chars[end - 1].0 == ' ' {
            end -= 1;
        }

        let mut line_spans = Vec::new();
        let mut cursor = line_start;
        while cursor < end {
            let style = chars[cursor].1.clone();
            let mut text = String::new();
            while cursor < end && chars[cursor].1 == &style {
                text.push(chars[cursor].0);
                cursor += 1;
            }
            line_spans.push(Span { text, style });
        }

        result.push(WrappedLine {
            spans: line_spans,
            start: cell_before[line_start],
            end: cell_before[end],
        });
        index = line_end;
    }

    if result.is_empty() {
        result.push(WrappedLine {
            spans: Vec::new(),
            start: 0,
            end: 0,
        });
    }
    result
}

fn is_image_asset(url: &str) -> bool {
    matches!(
        url.split('#')
            .next()
            .and_then(|path| std::path::Path::new(path).extension())
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg")
    )
}

fn wiki_link_target(target: &str, label: Option<String>) -> (String, LinkTarget) {
    match terminalwiki_core::link::WikiLink::parse_inner(target) {
        Some(wiki_link) => {
            let display = label.unwrap_or_else(|| wiki_link.display_text());
            let target = match wiki_link.target {
                terminalwiki_core::link::LinkTarget::Page { wiki, name, anchor } => {
                    LinkTarget::Wiki {
                        wiki,
                        page: name,
                        anchor,
                    }
                }
                terminalwiki_core::link::LinkTarget::File { path, lines, .. } => LinkTarget::File {
                    path: std::path::PathBuf::from(path),
                    line_range: lines.map(|lines| (lines.start as usize, lines.end as usize)),
                },
                terminalwiki_core::link::LinkTarget::Symbol { name } => LinkTarget::Wiki {
                    wiki: None,
                    page: name,
                    anchor: None,
                },
            };
            (display, target)
        }
        None => (
            label.unwrap_or_else(|| target.to_string()),
            LinkTarget::Wiki {
                wiki: None,
                page: target.to_string(),
                anchor: None,
            },
        ),
    }
}
