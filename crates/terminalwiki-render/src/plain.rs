use crate::document::RenderedDocument;

pub struct PlainRenderer;

impl PlainRenderer {
    pub fn render(doc: &RenderedDocument) -> String {
        let mut out = String::new();
        for line in &doc.lines {
            for span in line {
                out.push_str(&span.text);
            }
            out.push('\n');
        }
        out
    }
}
