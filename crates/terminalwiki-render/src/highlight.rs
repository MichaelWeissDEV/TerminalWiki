use crate::document::Span;
use crate::theme::{Theme, SemanticColor};

pub fn highlight(text: &str, lang: Option<&str>, theme: &Theme) -> Vec<Vec<Span>> {
    let mut lines = Vec::new();
    let is_code = lang.is_some();
    for line in text.lines() {
        let mut spans = Vec::new();
        if is_code {
            // Simple keyword highlighting placeholder
            for word in line.split_whitespace() {
                let style = if ["fn", "let", "mut", "if", "else", "return", "pub", "struct", "enum"].contains(&word) {
                    theme.style(SemanticColor::Keyword)
                } else {
                    theme.style(SemanticColor::Foreground)
                };
                spans.push(Span { text: word.to_string() + " ", style });
            }
        } else {
            spans.push(Span { text: line.to_string(), style: theme.style(SemanticColor::Foreground) });
        }
        lines.push(spans);
    }
    lines
}
