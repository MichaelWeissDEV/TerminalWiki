//! Semantic Document AST and Rendered Document types (spec §55, §56).

use crate::ansi::Style;

pub type RenderedLine = Vec<Span>;

/// A styled text segment in terminal output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

/// A rendered document ready for terminal display or TUI navigation.
#[derive(Debug, Clone, Default)]
pub struct RenderedDocument {
    pub lines: Vec<RenderedLine>,
    /// Headings: (level, text, line_index)
    pub headings: Vec<(usize, String, usize)>,
    /// Embedded links: (line_index, target_url_or_wiki_link)
    pub links: Vec<(usize, String)>,
}

/// Inline formatting elements.
#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Strike(Vec<Inline>),
    Code(String),
    Link { target: String, text: Vec<Inline> },
    WikiLink { target: String, label: Option<String> },
    Footnote(String),
    Math(String),
}

impl Inline {
    pub fn plain_text(&self) -> String {
        match self {
            Inline::Text(s) | Inline::Code(s) | Inline::Footnote(s) | Inline::Math(s) => s.clone(),
            Inline::Bold(inlines) | Inline::Italic(inlines) | Inline::Strike(inlines) => {
                inlines.iter().map(|i| i.plain_text()).collect()
            }
            Inline::Link { text, .. } => text.iter().map(|i| i.plain_text()).collect(),
            Inline::WikiLink { target, label } => label.clone().unwrap_or_else(|| target.clone()),
        }
    }
}

/// Block elements representing top-level document structure.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(usize, Vec<Inline>),
    Paragraph(Vec<Inline>),
    CodeBlock { language: Option<String>, content: String },
    List { ordered: bool, start: Option<u64>, items: Vec<Vec<Block>> },
    TaskItem { checked: bool, content: Vec<Inline> },
    BlockQuote(Vec<Block>),
    Callout { kind: String, title: Option<String>, content: Vec<Block> },
    Table { headers: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>> },
    HorizontalRule,
    Image { alt: String, url: String },
    Math(String),
}

/// Semantic Document model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub blocks: Vec<Block>,
}
