//! Rendering semantic Document AST into terminal Spans and RenderedLines (spec §55, §60).

use terminalwiki_core::unicode::{display_width, pad_display_width};

use crate::ansi::{ColorMode, Style};
use crate::document::{Block, Document, Inline, RenderedDocument, Span};
use crate::highlight::highlight;
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

        for (idx, block) in doc.blocks.iter().enumerate() {
            self.render_block(block, &mut rdoc, 0);
            if idx + 1 < doc.blocks.len() {
                rdoc.lines.push(Vec::new()); // blank line separation between top-level blocks
            }
        }

        rdoc
    }

    fn render_block(&self, block: &Block, rdoc: &mut RenderedDocument, indent_level: usize) {
        let indent = "  ".repeat(indent_level);

        match block {
            Block::Heading(level, inlines) => {
                let text = inlines.iter().map(|i| i.plain_text()).collect::<String>();
                rdoc.headings.push((*level, text.clone(), rdoc.lines.len()));

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
                let wrapped = self.wrap_spans(rendered_spans, self.max_content_width.saturating_sub(indent_level * 2));
                for line in wrapped {
                    let mut full_line = Vec::new();
                    if !indent.is_empty() {
                        full_line.push(Span { text: indent.clone(), style: Style::new() });
                    }
                    full_line.extend(line);
                    rdoc.lines.push(full_line);
                }
            }

            Block::CodeBlock { language, content } => {
                // Header bar
                let lang_label = language.as_deref().unwrap_or("code");
                rdoc.lines.push(vec![
                    Span { text: format!("{indent}```"), style: self.theme.style(SemanticColor::Muted) },
                    Span { text: lang_label.to_string(), style: self.theme.style(SemanticColor::Accent) },
                ]);

                let code_lines = highlight(content, language.as_deref(), self.theme);
                for line in code_lines {
                    let mut row = vec![Span { text: format!("{indent}  "), style: Style::new() }];
                    row.extend(line);
                    rdoc.lines.push(row);
                }

                rdoc.lines.push(vec![
                    Span { text: format!("{indent}```"), style: self.theme.style(SemanticColor::Muted) },
                ]);
            }

            Block::BlockQuote(blocks) => {
                let quote_prefix = format!("{indent}│ ");
                let quote_style = self.theme.style(SemanticColor::Muted);
                for b in blocks {
                    let mut sub_rdoc = RenderedDocument::default();
                    self.render_block(b, &mut sub_rdoc, 0);
                    for line in sub_rdoc.lines {
                        let mut row = vec![Span { text: quote_prefix.clone(), style: quote_style.clone() }];
                        row.extend(line);
                        rdoc.lines.push(row);
                    }
                }
            }

            Block::Callout { kind, title, content } => {
                let label = title.as_deref().unwrap_or(kind.as_str());
                let callout_style = self.theme.style(SemanticColor::Accent).bold();
                rdoc.lines.push(vec![
                    Span { text: format!("{indent}┌─ [!] "), style: callout_style.clone() },
                    Span { text: label.to_string(), style: callout_style },
                ]);

                for b in content {
                    let mut sub_rdoc = RenderedDocument::default();
                    self.render_block(b, &mut sub_rdoc, 0);
                    for line in sub_rdoc.lines {
                        let mut row = vec![Span { text: format!("{indent}│  "), style: self.theme.style(SemanticColor::Muted) }];
                        row.extend(line);
                        rdoc.lines.push(row);
                    }
                }
                rdoc.lines.push(vec![
                    Span { text: format!("{indent}└──"), style: self.theme.style(SemanticColor::Muted) },
                ]);
            }

            Block::List { ordered, start, items } => {
                for (i, item) in items.iter().enumerate() {
                    let marker = if *ordered {
                        let num = start.unwrap_or(1) + i as u64;
                        format!("{indent}{num}. ")
                    } else {
                        format!("{indent}• ")
                    };
                    let marker_style = self.theme.style(SemanticColor::Accent);

                    for (b_idx, b) in item.iter().enumerate() {
                        let mut sub_rdoc = RenderedDocument::default();
                        self.render_block(b, &mut sub_rdoc, 0);
                        for (l_idx, line) in sub_rdoc.lines.into_iter().enumerate() {
                            let mut row = Vec::new();
                            if b_idx == 0 && l_idx == 0 {
                                row.push(Span { text: marker.clone(), style: marker_style.clone() });
                            } else {
                                row.push(Span { text: " ".repeat(marker.len()), style: Style::new() });
                            }
                            row.extend(line);
                            rdoc.lines.push(row);
                        }
                    }
                }
            }

            Block::TaskItem { checked, content } => {
                let marker = if *checked { " [x] " } else { " [ ] " };
                let mut row = vec![Span { text: format!("{indent}{marker}"), style: self.theme.style(SemanticColor::Accent) }];
                row.extend(self.render_inlines(content, rdoc));
                rdoc.lines.push(row);
            }

            Block::Table { headers, rows } => {
                self.render_table(headers, rows, rdoc, indent_level);
            }

            Block::HorizontalRule => {
                let rule_width = self.max_content_width.min(60);
                rdoc.lines.push(vec![
                    Span { text: format!("{indent}{}", "─".repeat(rule_width)), style: self.theme.style(SemanticColor::Muted) },
                ]);
            }

            Block::Image { alt, url } => {
                rdoc.lines.push(vec![
                    Span { text: format!("{indent}[image: {alt} ({url})]"), style: self.theme.style(SemanticColor::Muted).italic() },
                ]);
            }

            Block::Math(formula) => {
                rdoc.lines.push(vec![
                    Span { text: format!("{indent}  ⟨ {formula} ⟩"), style: self.theme.style(SemanticColor::Code) },
                ]);
            }
        }
    }

    fn render_inlines(&self, inlines: &[Inline], rdoc: &mut RenderedDocument) -> Vec<Span> {
        let mut spans = Vec::new();
        for inline in inlines {
            match inline {
                Inline::Text(t) => {
                    spans.push(Span { text: t.clone(), style: self.theme.style(SemanticColor::Foreground) });
                }
                Inline::Bold(inner) => {
                    for mut s in self.render_inlines(inner, rdoc) {
                        s.style = s.style.bold();
                        spans.push(s);
                    }
                }
                Inline::Italic(inner) => {
                    for mut s in self.render_inlines(inner, rdoc) {
                        s.style = s.style.italic();
                        spans.push(s);
                    }
                }
                Inline::Strike(inner) => {
                    for mut s in self.render_inlines(inner, rdoc) {
                        s.style.strikethrough = true;
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
                    rdoc.links.push((line_no, target.clone()));
                    for mut s in self.render_inlines(text, rdoc) {
                        s.style = self.theme.style(SemanticColor::Link).underline();
                        spans.push(s);
                    }
                }
                Inline::WikiLink { target, label } => {
                    let line_no = rdoc.lines.len();
                    rdoc.links.push((line_no, target.clone()));
                    let display_text = label.as_deref().unwrap_or(target.as_str());
                    spans.push(Span {
                        text: format!("[[{display_text}]]"),
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
                    spans.push(Span {
                        text: format!("⟨{m}⟩"),
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
        let num_cols = headers.len().max(rows.iter().map(|r| r.len()).max().unwrap_or(0));
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
                    let w = display_width(&cell.iter().map(|in_l| in_l.plain_text()).collect::<String>());
                    col_widths[i] = col_widths[i].max(w);
                }
            }
        }

        // Render header row
        let mut header_spans = vec![Span { text: indent.clone(), style: Style::new() }];
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
            Span { text: indent.clone(), style: Style::new() },
            Span {
                text: "─".repeat(total_rule_len),
                style: self.theme.style(SemanticColor::Muted),
            },
        ]);

        // Render data rows
        for row in rows {
            let mut row_spans = vec![Span { text: indent.clone(), style: Style::new() }];
            for (i, cell) in row.iter().enumerate() {
                if i < num_cols {
                    let plain = cell.iter().map(|in_l| in_l.plain_text()).collect::<String>();
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
