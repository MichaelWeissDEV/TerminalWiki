use crate::document::{RenderedDocument, Span};
use crate::theme::{SemanticColor, Theme};

pub fn render_binary_info(path: &std::path::Path, size: u64, _mtime: u64) -> RenderedDocument {
    let mut doc = RenderedDocument {
        lines: Vec::new(),
        headings: Vec::new(),
        links: Vec::new(),
    };
    let theme = Theme::Dark; // Default fallback
    doc.lines.push(vec![Span {
        text: format!("Binary file: {} ({} bytes)", path.display(), size),
        style: theme.style(SemanticColor::Foreground),
    }]);
    doc
}
