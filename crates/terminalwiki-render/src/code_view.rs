use crate::document::{RenderedDocument, Span};
use crate::ansi::ColorMode;
use crate::theme::{Theme, SemanticColor};

pub fn render_code_file(
    content: &str,
    _language: Option<&str>,
    _path: &std::path::Path,
    _config: &terminalwiki_core::config::Config,
    theme: &Theme,
    _color_mode: ColorMode,
    _highlight_lines: Option<(usize, usize)>,
) -> RenderedDocument {
    let mut doc = RenderedDocument {
        lines: Vec::new(),
        headings: Vec::new(),
        links: Vec::new(),
    };
    for (i, line) in content.lines().enumerate() {
        let mut span = vec![Span { text: format!("{:4} | ", i + 1), style: theme.style(SemanticColor::Muted) }];
        span.push(Span { text: line.to_string(), style: theme.style(SemanticColor::Code) });
        doc.lines.push(span);
    }
    doc
}
