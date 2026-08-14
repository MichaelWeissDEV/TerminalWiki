//! Semantic Document AST and Rendered Document types (spec §55, §56).

use crate::ansi::Style;
use std::path::PathBuf;

pub type RenderedLine = Vec<Span>;

/// A styled text segment in terminal output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

/// Target type of a rendered link (spec §32, §34).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    Wiki {
        wiki: Option<String>,
        page: String,
        anchor: Option<String>,
    },
    File {
        path: PathBuf,
        line_range: Option<(usize, usize)>,
    },
    External(String),
    Heading(String),
}

/// A rendered link with visual position and target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedLink {
    pub line: usize,
    pub start_column: usize,
    pub end_column: usize,
    pub target: LinkTarget,
    pub label: String,
}

/// Heading metadata for navigation and outline views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedHeading {
    pub level: usize,
    pub text: String,
    pub line: usize,
}

/// A rendered document ready for terminal display or TUI navigation.
#[derive(Debug, Clone, Default)]
pub struct RenderedDocument {
    pub lines: Vec<RenderedLine>,
    pub headings: Vec<RenderedHeading>,
    pub links: Vec<RenderedLink>,
}

/// Inline formatting elements.
#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Strike(Vec<Inline>),
    Code(String),
    Link {
        target: String,
        text: Vec<Inline>,
    },
    WikiLink {
        target: String,
        label: Option<String>,
    },
    Footnote(String),
    Math(String),
}

impl Inline {
    pub fn plain_text(&self) -> String {
        match self {
            Inline::Text(t) | Inline::Code(t) | Inline::Footnote(t) | Inline::Math(t) => t.clone(),
            Inline::Bold(inner)
            | Inline::Italic(inner)
            | Inline::Strike(inner)
            | Inline::Link { text: inner, .. } => inner.iter().map(|i| i.plain_text()).collect(),
            Inline::WikiLink { target, label } => label.clone().unwrap_or_else(|| target.clone()),
        }
    }
}

/// Block-level formatting elements.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(usize, Vec<Inline>),
    Paragraph(Vec<Inline>),
    CodeBlock {
        language: Option<String>,
        content: String,
    },
    List {
        ordered: bool,
        start: u64,
        items: Vec<Vec<Block>>,
    },
    TaskItem {
        checked: bool,
        content: Vec<Inline>,
    },
    BlockQuote(Vec<Block>),
    Table {
        headers: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    HorizontalRule,
    Image {
        alt: String,
        url: String,
    },
    Callout {
        kind: String,
        title: Option<String>,
        content: Vec<Block>,
    },
    Math(String),
}

/// A parsed document represented as a list of blocks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub blocks: Vec<Block>,
}
