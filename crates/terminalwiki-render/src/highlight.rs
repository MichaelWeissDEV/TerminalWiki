//! Syntax highlighting powered by syntect (spec §27, §30).
//!
//! Lazily loads syntax sets and themes to keep initial startup cost near zero.
//! Preserves all whitespace, indentation, and column structure accurately.

use once_cell::sync::Lazy;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color as SynColor, FontStyle, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::ansi::{Color, Style};
use crate::document::Span;
use crate::theme::{SemanticColor, Theme};

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

fn find_syntax(lang_or_ext: Option<&str>) -> Option<&'static SyntaxReference> {
    let name = lang_or_ext?.trim();
    if name.is_empty() {
        return None;
    }
    SYNTAX_SET
        .find_syntax_by_token(name)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(name))
}

fn syn_to_style(syn_color: SynColor, font_style: FontStyle) -> Style {
    let mut style = Style::new().fg(Color::Rgb(syn_color.r, syn_color.g, syn_color.b));
    if font_style.contains(FontStyle::BOLD) {
        style = style.bold();
    }
    if font_style.contains(FontStyle::ITALIC) {
        style = style.italic();
    }
    if font_style.contains(FontStyle::UNDERLINE) {
        style = style.underline();
    }
    style
}

/// Highlights source code, returning lines of styled spans with whitespace strictly preserved.
pub fn highlight(text: &str, lang_or_ext: Option<&str>, theme: &Theme) -> Vec<Vec<Span>> {
    let syntax = find_syntax(lang_or_ext).unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    // Select suitable syntect theme based on active Theme variant
    let theme_name = match theme {
        Theme::Light => "base16-ocean.light",
        Theme::Mono => "base16-ocean.dark",
        _ => "base16-ocean.dark",
    };
    let syn_theme = &THEME_SET.themes[theme_name];

    let mut highlighter = HighlightLines::new(syntax, syn_theme);
    let mut output_lines = Vec::new();

    for line in text.lines() {
        let line_with_nl = format!("{line}\n");
        if let Ok(ranges) = highlighter.highlight_line(&line_with_nl, &SYNTAX_SET) {
            let mut spans = Vec::new();
            for (style, text_slice) in ranges {
                let trimmed = text_slice.trim_end_matches(['\r', '\n']);
                if !trimmed.is_empty() {
                    spans.push(Span {
                        text: trimmed.to_string(),
                        style: syn_to_style(style.foreground, style.font_style),
                    });
                }
            }
            output_lines.push(spans);
        } else {
            // Fallback plain line
            output_lines.push(vec![Span {
                text: line.to_string(),
                style: theme.style(SemanticColor::Foreground),
            }]);
        }
    }

    if output_lines.is_empty() {
        output_lines.push(Vec::new());
    }

    output_lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_code_preserving_whitespace() {
        let code = "fn main() {\n    let   x = 42;\n}";
        let lines = highlight(code, Some("rs"), &Theme::Dark);
        assert_eq!(lines.len(), 3);
        // Ensure indentation is preserved in spans
        let line2_text: String = lines[1].iter().map(|s| s.text.as_str()).collect();
        assert_eq!(line2_text, "    let   x = 42;");
    }

    #[test]
    fn handles_unknown_syntax_gracefully() {
        let code = "some unknown code content";
        let lines = highlight(code, Some("unknown_ext_xyz"), &Theme::Dark);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0][0].text, code);
    }
}
