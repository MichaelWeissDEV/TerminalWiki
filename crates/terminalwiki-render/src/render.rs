use crate::ansi::ColorMode;
use crate::document::{Document, Block, RenderedDocument, Span};
use crate::theme::{Theme, SemanticColor};
use crate::width::wrap_text;

pub struct Renderer<'a> {
    max_content_width: usize,
    theme: &'a Theme,
    _color_mode: ColorMode,
}

impl<'a> Renderer<'a> {
    pub fn new(max_content_width: usize, theme: &'a Theme, _color_mode: ColorMode) -> Self {
        Self { max_content_width, theme, _color_mode }
    }

    pub fn render_document(&self, doc: &Document) -> RenderedDocument {
        let mut rdoc = RenderedDocument {
            lines: Vec::new(),
            headings: Vec::new(),
            links: Vec::new(),
        };

        for block in &doc.blocks {
            match block {
                Block::Heading(level, text) => {
                    rdoc.headings.push((*level, text.clone(), rdoc.lines.len()));
                    let style = self.theme.style(SemanticColor::Heading).with_bold();
                    let prefix = "#".repeat(*level);
                    rdoc.lines.push(vec![Span { text: format!("{} {}", prefix, text), style }]);
                }
                Block::Paragraph(text) => {
                    for line in wrap_text(text, self.max_content_width) {
                        rdoc.lines.push(vec![Span { text: line, style: self.theme.style(SemanticColor::Foreground) }]);
                    }
                }
                Block::CodeBlock(lang, text) => {
                    for line in crate::highlight::highlight(text, lang.as_deref(), self.theme) {
                        rdoc.lines.push(line);
                    }
                }
                Block::Callout(ctype, text) => {
                    let style = self.theme.style(SemanticColor::Accent);
                    rdoc.lines.push(vec![Span { text: format!("> [{}]", ctype), style: style.clone() }]);
                    for line in wrap_text(text, self.max_content_width - 2) {
                        rdoc.lines.push(vec![Span { text: format!("> {}", line), style: self.theme.style(SemanticColor::Foreground) }]);
                    }
                }
                _ => {}
            }
            rdoc.lines.push(vec![]); // empty line
        }

        rdoc
    }
}
