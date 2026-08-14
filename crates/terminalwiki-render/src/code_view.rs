//! Code file rendering and line addressing (spec §27, §30).

use std::ops::RangeInclusive;
use std::path::Path;

use terminalwiki_core::unicode::display_width;

use crate::ansi::{Color, ColorMode, Style};
use crate::document::{RenderedDocument, Span};
use crate::highlight::highlight;
use crate::theme::{SemanticColor, Theme};

/// Configuration options for rendering source code files.
#[derive(Debug, Clone)]
pub struct CodeRenderOptions {
    pub line_numbers: bool,
    pub start_line: usize,
    pub highlight_range: Option<RangeInclusive<usize>>,
    pub tab_width: usize,
    pub max_width: usize,
}

impl Default for CodeRenderOptions {
    fn default() -> Self {
        Self {
            line_numbers: true,
            start_line: 1,
            highlight_range: None,
            tab_width: 4,
            max_width: 80,
        }
    }
}

/// Renders a code file into a `RenderedDocument` with optional header bar, syntax highlighting and line numbers.
pub fn render_code_file(
    content: &str,
    language: Option<&str>,
    path: &Path,
    config: &terminalwiki_core::config::Config,
    theme: &Theme,
    _color_mode: ColorMode,
    highlight_lines: Option<(usize, usize)>,
) -> RenderedDocument {
    let highlight_range = highlight_lines.map(|(s, e)| s..=e);
    let options = CodeRenderOptions {
        line_numbers: config.render.line_numbers,
        start_line: 1,
        highlight_range,
        tab_width: config.render.tab_width.max(1),
        max_width: config.render.max_content_width.max(40),
    };
    render_code_with_options(content, language, path, &options, theme)
}

/// Renders code with explicit `CodeRenderOptions`.
pub fn render_code_with_options(
    content: &str,
    language: Option<&str>,
    path: &Path,
    options: &CodeRenderOptions,
    theme: &Theme,
) -> RenderedDocument {
    let mut doc = RenderedDocument {
        lines: Vec::new(),
        headings: Vec::new(),
        links: Vec::new(),
    };

    // 1. Header Bar: `path.display()                           Language`
    let path_str = path.display().to_string();
    let lang_str = language.unwrap_or("Code");
    let header_width = options.max_width.max(40);
    let path_w = display_width(&path_str);
    let lang_w = display_width(lang_str);
    let pad_len = header_width.saturating_sub(path_w + lang_w);

    let header_span = vec![
        Span {
            text: path_str,
            style: theme.style(SemanticColor::Foreground).bold(),
        },
        Span {
            text: " ".repeat(pad_len.max(2)),
            style: Style::new(),
        },
        Span {
            text: lang_str.to_string(),
            style: theme.style(SemanticColor::Muted),
        },
    ];
    doc.lines.push(header_span);

    // Separator rule
    let sep_style = theme.style(SemanticColor::Muted);
    doc.lines.push(vec![Span {
        text: "─".repeat(header_width),
        style: sep_style,
    }]);

    // 2. Syntax-highlighted body lines
    let highlighted_lines = highlight(content, language, theme);
    let total_lines = highlighted_lines.len();
    let line_num_width = format!("{}", options.start_line + total_lines).len().max(2);

    for (idx, line_spans) in highlighted_lines.into_iter().enumerate() {
        let line_no = options.start_line + idx;
        let is_highlighted = options
            .highlight_range
            .as_ref()
            .is_some_and(|r| r.contains(&line_no));

        let mut row_spans = Vec::new();
        let mut current_col = 0;

        // Line number prefix
        if options.line_numbers {
            let num_style = if is_highlighted {
                theme.style(SemanticColor::Accent).bold()
            } else {
                theme.style(SemanticColor::Muted)
            };
            let prefix = if is_highlighted {
                format!("{:>width$} > ", line_no, width = line_num_width)
            } else {
                format!("{:>width$} │ ", line_no, width = line_num_width)
            };
            current_col += display_width(&prefix);
            row_spans.push(Span {
                text: prefix,
                style: num_style,
            });
        }

        // Expand tabs with true tabstop alignment based on display column
        for mut span in line_spans {
            if span.text.contains('\t') {
                span.text = expand_tabs_with_column(&span.text, options.tab_width, &mut current_col);
            } else {
                current_col += display_width(&span.text);
            }
            if is_highlighted {
                span.style.fg = span.style.fg.or(Some(Color::BrightWhite));
            }
            row_spans.push(span);
        }

        doc.lines.push(row_spans);
    }

    doc
}

fn expand_tabs_with_column(text: &str, tab_width: usize, current_col: &mut usize) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == '\t' {
            let spaces = tab_width - (*current_col % tab_width);
            for _ in 0..spaces {
                out.push(' ');
            }
            *current_col += spaces;
        } else {
            out.push(ch);
            *current_col += display_width(&ch.to_string());
        }
    }
    out
}
