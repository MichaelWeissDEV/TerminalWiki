//! Rendering semantic Document AST into terminal Spans and RenderedLines (spec §55, §60).

use std::collections::HashMap;

use terminalwiki_core::link::heading_anchor;
use terminalwiki_core::unicode::{display_width, pad_display_width};

use crate::ansi::{ColorMode, Style};
use crate::document::{
    Block, Document, Inline, LinkTarget, RenderedDocument, RenderedHeading, RenderedLink, Span,
};
use crate::highlight::highlight;
use crate::math::layout_math;
use crate::theme::{SemanticColor, Theme};
use crate::width::wrap_text;

pub struct Renderer<'a> {
    max_content_width: usize,
    theme: &'a Theme,
    _color_mode: ColorMode,
}

impl<'a> Renderer<'a> {
    pub fn new(max_content_width: usize, theme: &'a Theme, color_mode: ColorMode) -> Self {
        Self {
            max_content_width: max_content_width.max(40),
            theme,
            _color_mode: color_mode,
        }
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
                let rendered_spans = self.render_inlines(inlines, rdoc);
                let wrapped = self.wrap_spans(
                    rendered_spans,
                    self.max_content_width.saturating_sub(indent_level * 2),
                );
                for line in wrapped {
                    let mut full_line = Vec::new();
                    if !indent.is_empty() {
                        full_line.push(Span {
                            text: indent.clone(),
                            style: Style::new(),
                        });
                    }
                    full_line.extend(line);
                    rdoc.lines.push(full_line);
                }
            }

            Block::CodeBlock { language, content } => {
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
                let quote_prefix = format!("{indent}│ ");
                let quote_style = self.theme.style(SemanticColor::Muted);
                for b in blocks {
                    let mut sub_rdoc = RenderedDocument::default();
                    self.render_block(b, &mut sub_rdoc, 0, heading_anchors);
                    for line in sub_rdoc.lines {
                        let mut row = vec![Span {
                            text: quote_prefix.clone(),
                            style: quote_style.clone(),
                        }];
                        row.extend(line);
                        rdoc.lines.push(row);
                    }
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

                for b in content {
                    let mut sub_rdoc = RenderedDocument::default();
                    self.render_block(b, &mut sub_rdoc, 0, heading_anchors);
                    for line in sub_rdoc.lines {
                        let mut row = vec![Span {
                            text: format!("{indent}│  "),
                            style: self.theme.style(SemanticColor::Muted),
                        }];
                        row.extend(line);
                        rdoc.lines.push(row);
                    }
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
                for block in content {
                    let mut sub_rdoc = RenderedDocument::default();
                    self.render_block(block, &mut sub_rdoc, 0, heading_anchors);
                    for line in sub_rdoc.lines {
                        let mut row = vec![Span {
                            text: format!("{indent}  "),
                            style: footnote_style.clone(),
                        }];
                        row.extend(line);
                        rdoc.lines.push(row);
                    }
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
                    let mut task_content: Option<Vec<Span>> = None;
                    if let Some(Block::TaskItem { checked, content }) = item.first() {
                        let glyph = if *checked { "☑" } else { "☐" };
                        marker = format!("{indent}{glyph} ");
                        task_content = Some(self.render_inlines(content, rdoc));
                        body = &item[1..];
                    }

                    let padding = " ".repeat(display_width(&marker));
                    let mut emitted_marker = false;

                    if let Some(content) = task_content {
                        let mut row = vec![Span {
                            text: marker.clone(),
                            style: marker_style.clone(),
                        }];
                        row.extend(content);
                        rdoc.lines.push(row);
                        emitted_marker = true;
                    }

                    for b in body {
                        let mut sub_rdoc = RenderedDocument::default();
                        self.render_block(b, &mut sub_rdoc, 0, heading_anchors);
                        for line in sub_rdoc.lines {
                            let mut row = Vec::new();
                            if emitted_marker {
                                row.push(Span {
                                    text: padding.clone(),
                                    style: Style::new(),
                                });
                            } else {
                                row.push(Span {
                                    text: marker.clone(),
                                    style: marker_style.clone(),
                                });
                                emitted_marker = true;
                            }
                            row.extend(line);
                            rdoc.lines.push(row);
                        }
                    }
                }
            }

            Block::TaskItem { checked, content } => {
                let marker = if *checked { " [x] " } else { " [ ] " };
                let mut row = vec![Span {
                    text: format!("{indent}{marker}"),
                    style: self.theme.style(SemanticColor::Accent),
                }];
                row.extend(self.render_inlines(content, rdoc));
                rdoc.lines.push(row);
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
                    rdoc.lines.push(vec![Span {
                        text: format!("{indent}[image: {alt} ({url})]"),
                        style: self.theme.style(SemanticColor::Muted).italic(),
                    }]);
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

    fn render_inlines(&self, inlines: &[Inline], rdoc: &mut RenderedDocument) -> Vec<Span> {
        self.render_inlines_at(inlines, rdoc, 0)
    }

    /// Renders an inline run while retaining the absolute cell offset used by
    /// link hit regions. Formatting nodes recurse with their current offset,
    /// so links after text or inside emphasis are addressable precisely.
    fn render_inlines_at(
        &self,
        inlines: &[Inline],
        rdoc: &mut RenderedDocument,
        base_column: usize,
    ) -> Vec<Span> {
        let mut spans: Vec<Span> = Vec::new();
        for inline in inlines {
            let start_column = base_column
                + spans
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
                    for mut s in self.render_inlines_at(inner, rdoc, start_column) {
                        s.style = s.style.bold();
                        spans.push(s);
                    }
                }
                Inline::Italic(inner) => {
                    for mut s in self.render_inlines_at(inner, rdoc, start_column) {
                        s.style = s.style.italic();
                        spans.push(s);
                    }
                }
                Inline::Strike(inner) => {
                    for mut s in self.render_inlines_at(inner, rdoc, start_column) {
                        s.style.strikethrough = true;
                        spans.push(s);
                    }
                }
                Inline::Highlight(inner) => {
                    for mut s in self.render_inlines_at(inner, rdoc, start_column) {
                        s.style = self.theme.style(SemanticColor::Selection).bold();
                        spans.push(s);
                    }
                }
                Inline::Code(c) => {
                    spans.push(Span {
                        text: format!("`{c}`"),
                        style: self.theme.style(SemanticColor::Code),
                    });
                }
                Inline::Link { target, text } => {
                    let line_no = rdoc.lines.len();
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
                    rdoc.links.push(RenderedLink {
                        line: line_no,
                        start_column,
                        end_column: start_column + display_width(&label),
                        target: link_target,
                        label: label.clone(),
                    });
                    for mut s in self.render_inlines_at(text, rdoc, start_column) {
                        s.style = self.theme.style(SemanticColor::Link).underline();
                        spans.push(s);
                    }
                }
                Inline::WikiLink { target, label } => {
                    let line_no = rdoc.lines.len();
                    let (display_text, link_target) = wiki_link_target(target, label.clone());

                    let full_display = format!("[[{display_text}]]");

                    rdoc.links.push(RenderedLink {
                        line: line_no,
                        start_column,
                        end_column: start_column + display_width(&full_display),
                        target: link_target,
                        label: display_text,
                    });
                    spans.push(Span {
                        text: full_display,
                        style: self.theme.style(SemanticColor::Link).bold(),
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
        spans
    }

    fn render_table(
        &self,
        headers: &[Vec<Inline>],
        rows: &[Vec<Vec<Inline>>],
        rdoc: &mut RenderedDocument,
        indent_level: usize,
    ) {
        let indent = "  ".repeat(indent_level);
        let num_cols = headers
            .len()
            .max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
        if num_cols == 0 {
            return;
        }

        // Calculate column widths
        let mut col_widths = vec![4usize; num_cols];
        for (i, h) in headers.iter().enumerate() {
            let w = display_width(&h.iter().map(|in_l| in_l.plain_text()).collect::<String>());
            col_widths[i] = col_widths[i].max(w);
        }
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                if i < num_cols {
                    let w = display_width(
                        &cell
                            .iter()
                            .map(|in_l| in_l.plain_text())
                            .collect::<String>(),
                    );
                    col_widths[i] = col_widths[i].max(w);
                }
            }
        }

        // Render header row
        let mut header_spans = vec![Span {
            text: indent.clone(),
            style: Style::new(),
        }];
        for (i, h) in headers.iter().enumerate() {
            let plain = h.iter().map(|in_l| in_l.plain_text()).collect::<String>();
            let padded = pad_display_width(&plain, col_widths[i] + 2);
            header_spans.push(Span {
                text: padded,
                style: self.theme.style(SemanticColor::Heading).bold(),
            });
        }
        rdoc.lines.push(header_spans);

        // Separator rule
        let total_rule_len: usize = col_widths.iter().map(|w| w + 2).sum();
        rdoc.lines.push(vec![
            Span {
                text: indent.clone(),
                style: Style::new(),
            },
            Span {
                text: "─".repeat(total_rule_len),
                style: self.theme.style(SemanticColor::Muted),
            },
        ]);

        // Render data rows
        for row in rows {
            let mut row_spans = vec![Span {
                text: indent.clone(),
                style: Style::new(),
            }];
            for (i, cell) in row.iter().enumerate() {
                if i < num_cols {
                    let plain = cell
                        .iter()
                        .map(|in_l| in_l.plain_text())
                        .collect::<String>();
                    let padded = pad_display_width(&plain, col_widths[i] + 2);
                    row_spans.push(Span {
                        text: padded,
                        style: self.theme.style(SemanticColor::Foreground),
                    });
                }
            }
            rdoc.lines.push(row_spans);
        }
    }

    fn wrap_spans(&self, spans: Vec<Span>, max_width: usize) -> Vec<Vec<Span>> {
        let plain_text: String = spans.iter().map(|s| s.text.as_str()).collect();
        let wrapped_lines = wrap_text(&plain_text, max_width);
        if wrapped_lines.len() <= 1 {
            return vec![spans];
        }

        let mut result = Vec::new();
        for line in wrapped_lines {
            result.push(vec![Span {
                text: line,
                style: self.theme.style(SemanticColor::Foreground),
            }]);
        }
        result
    }
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
