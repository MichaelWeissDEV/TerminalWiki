use crate::ansi::Style;

pub type RenderedLine = Vec<Span>;

#[derive(Debug, Clone)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

#[derive(Debug, Clone)]
pub struct RenderedDocument {
    pub lines: Vec<RenderedLine>,
    pub headings: Vec<(usize, String, usize)>, // (level, text, line_index)
    pub links: Vec<(usize, String)>,           // (line_index, url/target)
}

#[derive(Debug, Clone)]
pub enum Block {
    Heading(usize, String),
    Paragraph(String),
    CodeBlock(Option<String>, String),
    Table(Vec<String>, Vec<Vec<String>>), // Headers, Rows
    BlockQuote(String),
    HorizontalRule,
    Callout(String, String), // Type, Content
    Math(String),
}

#[derive(Debug, Clone, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
}
