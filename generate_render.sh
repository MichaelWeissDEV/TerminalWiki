#!/bin/bash

# src/lib.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/lib.rs
pub mod ansi;
pub mod binary;
pub mod code_view;
pub mod document;
pub mod highlight;
pub mod markdown;
pub mod math;
pub mod plain;
pub mod render;
pub mod theme;
pub mod width;

pub use ansi::{Color, ColorMode, Style};
pub use binary::render_binary_info;
pub use code_view::render_code_file;
pub use document::{Document, RenderedDocument, RenderedLine, Span};
pub use markdown::parse_markdown;
pub use math::MathRenderer;
pub use plain::PlainRenderer;
pub use render::Renderer;
pub use theme::{SemanticColor, Theme};

use std::io::Write;

/// Detect if output should use color
pub fn detect_color_mode() -> ColorMode {
    if std::env::var("NO_COLOR").is_ok() {
        return ColorMode::Never;
    }
    ColorMode::Auto
}

/// Main entry point for rendering a markdown page
pub fn render_markdown(
    text: &str,
    config: &terminalwiki_core::config::Config,
    theme: &Theme,
    color_mode: ColorMode,
) -> RenderedDocument {
    let doc = markdown::parse_markdown(text);
    let renderer = render::Renderer::new(config.render.max_content_width, theme, color_mode);
    renderer.render_document(&doc)
}

/// Write rendered document to a Writer (stdout, stderr, buffer)
pub fn write_document(doc: &RenderedDocument, out: &mut impl Write, color: ColorMode) -> std::io::Result<()> {
    for line in &doc.lines {
        for span in line {
            if color == ColorMode::Never {
                write!(out, "{}", span.text)?;
            } else {
                write!(out, "{}", span.style.apply(&span.text))?;
            }
        }
        writeln!(out)?;
    }
    Ok(())
}
INNER_EOF

# src/ansi.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/ansi.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode { Auto, Always, Never }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Reset, Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
    BrightBlack, BrightRed, BrightGreen, BrightYellow, BrightBlue, BrightMagenta, BrightCyan, BrightWhite,
    Rgb(u8, u8, u8), Index(u8),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

impl Style {
    pub fn new() -> Self { Self::default() }
    pub fn fg(mut self, c: Color) -> Self { self.fg = Some(c); self }
    pub fn bold(mut self) -> Self { self.bold = true; self }
    pub fn italic(mut self) -> Self { self.italic = true; self }
    pub fn dim(mut self) -> Self { self.dim = true; self }
    pub fn underline(mut self) -> Self { self.underline = true; self }

    pub fn apply(&self, text: &str) -> String {
        let mut prefix = String::new();
        if self.bold { prefix.push_str("\x1b[1m"); }
        if self.dim { prefix.push_str("\x1b[2m"); }
        if self.italic { prefix.push_str("\x1b[3m"); }
        if self.underline { prefix.push_str("\x1b[4m"); }
        if self.strikethrough { prefix.push_str("\x1b[9m"); }
        if let Some(c) = self.fg {
            match c {
                Color::Reset => prefix.push_str("\x1b[39m"),
                Color::Black => prefix.push_str("\x1b[30m"),
                Color::Red => prefix.push_str("\x1b[31m"),
                Color::Green => prefix.push_str("\x1b[32m"),
                Color::Yellow => prefix.push_str("\x1b[33m"),
                Color::Blue => prefix.push_str("\x1b[34m"),
                Color::Magenta => prefix.push_str("\x1b[35m"),
                Color::Cyan => prefix.push_str("\x1b[36m"),
                Color::White => prefix.push_str("\x1b[37m"),
                Color::BrightBlack => prefix.push_str("\x1b[90m"),
                Color::BrightRed => prefix.push_str("\x1b[91m"),
                Color::BrightGreen => prefix.push_str("\x1b[92m"),
                Color::BrightYellow => prefix.push_str("\x1b[93m"),
                Color::BrightBlue => prefix.push_str("\x1b[94m"),
                Color::BrightMagenta => prefix.push_str("\x1b[95m"),
                Color::BrightCyan => prefix.push_str("\x1b[96m"),
                Color::BrightWhite => prefix.push_str("\x1b[97m"),
                Color::Rgb(r, g, b) => prefix.push_str(&format!("\x1b[38;2;{};{};{}m", r, g, b)),
                Color::Index(i) => prefix.push_str(&format!("\x1b[38;5;{}m", i)),
            }
        }
        let suffix = if !prefix.is_empty() { "\x1b[0m" } else { "" };
        format!("{}{}{}", prefix, terminalwiki_core::sanitize::sanitize(text), suffix)
    }
}
INNER_EOF

# src/theme.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/theme.rs
use crate::ansi::{Color, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticColor {
    Foreground, Muted, Heading, Link, Code, Selection, Warning, Error, Success, Accent, Keyword, String, Comment, Number, Function
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Theme { Dark, Light, Mono }

impl Theme {
    pub fn color(&self, semantic: SemanticColor) -> Color {
        match self {
            Theme::Dark => match semantic {
                SemanticColor::Foreground => Color::White,
                SemanticColor::Muted => Color::BrightBlack,
                SemanticColor::Heading => Color::Cyan,
                SemanticColor::Link => Color::Blue,
                SemanticColor::Code => Color::Yellow,
                SemanticColor::Selection => Color::Magenta,
                SemanticColor::Warning => Color::BrightYellow,
                SemanticColor::Error => Color::Red,
                SemanticColor::Success => Color::Green,
                SemanticColor::Accent => Color::Cyan,
                SemanticColor::Keyword => Color::Magenta,
                SemanticColor::String => Color::Green,
                SemanticColor::Comment => Color::BrightBlack,
                SemanticColor::Number => Color::Yellow,
                SemanticColor::Function => Color::Blue,
            },
            Theme::Light => match semantic {
                SemanticColor::Foreground => Color::Black,
                SemanticColor::Muted => Color::BrightBlack,
                SemanticColor::Heading => Color::Cyan,
                SemanticColor::Link => Color::Blue,
                SemanticColor::Code => Color::Yellow,
                SemanticColor::Selection => Color::Magenta,
                SemanticColor::Warning => Color::BrightYellow,
                SemanticColor::Error => Color::Red,
                SemanticColor::Success => Color::Green,
                SemanticColor::Accent => Color::Cyan,
                SemanticColor::Keyword => Color::Magenta,
                SemanticColor::String => Color::Green,
                SemanticColor::Comment => Color::BrightBlack,
                SemanticColor::Number => Color::Yellow,
                SemanticColor::Function => Color::Blue,
            },
            Theme::Mono => Color::Reset,
        }
    }
    pub fn style(&self, semantic: SemanticColor) -> Style {
        Style::new().fg(self.color(semantic))
    }
}
INNER_EOF

# src/document.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/document.rs
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
INNER_EOF

# src/markdown.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/markdown.rs
use pulldown_cmark::{Parser, Event, Tag, TagEnd, Options, CodeBlockKind};
use crate::document::{Document, Block};

pub fn parse_markdown(text: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    // pulldown-cmark doesn't have math natively without extensions but we can parse simple math from text if needed, or assume text contains it.
    // Spec says use simple math fallback.
    let parser = Parser::new_ext(text, options);
    
    let mut doc = Document::default();
    let mut current_text = String::new();
    let mut in_heading = None;
    let mut in_paragraph = false;
    let mut in_code_block = None;
    
    // Very basic parsing for now.
    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = Some(level as usize);
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = in_heading {
                    doc.blocks.push(Block::Heading(level, current_text.clone()));
                    current_text.clear();
                }
                in_heading = None;
            }
            Event::Start(Tag::Paragraph) => {
                in_paragraph = true;
            }
            Event::End(TagEnd::Paragraph) => {
                if current_text.starts_with("[!NOTE]") {
                    doc.blocks.push(Block::Callout("NOTE".to_string(), current_text[7..].trim().to_string()));
                } else {
                    doc.blocks.push(Block::Paragraph(current_text.clone()));
                }
                current_text.clear();
                in_paragraph = false;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => Some(l.to_string()),
                    _ => None,
                };
                in_code_block = Some(lang);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(lang) = in_code_block.take() {
                    doc.blocks.push(Block::CodeBlock(lang, current_text.clone()));
                    current_text.clear();
                }
            }
            Event::Text(t) => {
                current_text.push_str(&t);
            }
            Event::Code(t) => {
                current_text.push('`');
                current_text.push_str(&t);
                current_text.push('`');
            }
            _ => {}
        }
    }
    
    doc
}
INNER_EOF

# src/highlight.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/highlight.rs
use crate::ansi::Style;
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
INNER_EOF

# src/render.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/render.rs
use crate::ansi::{ColorMode, Style};
use crate::document::{Document, Block, RenderedDocument, Span};
use crate::theme::{Theme, SemanticColor};
use crate::width::wrap_text;

pub struct Renderer<'a> {
    max_content_width: usize,
    theme: &'a Theme,
    color_mode: ColorMode,
}

impl<'a> Renderer<'a> {
    pub fn new(max_content_width: usize, theme: &'a Theme, color_mode: ColorMode) -> Self {
        Self { max_content_width, theme, color_mode }
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
                    let style = self.theme.style(SemanticColor::Heading).bold();
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
INNER_EOF

# src/math.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/math.rs
pub trait MathRenderer {
    fn render_inline(&self, formula: &str) -> String;
    fn render_display(&self, formula: &str) -> Vec<String>;
}

pub struct TerminalTextMathRenderer;

impl MathRenderer for TerminalTextMathRenderer {
    fn render_inline(&self, formula: &str) -> String {
        format!("⟨{}⟩", formula)
    }

    fn render_display(&self, formula: &str) -> Vec<String> {
        formula.lines().map(|l| format!("    {}", l)).collect()
    }
}
INNER_EOF

# src/plain.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/plain.rs
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
INNER_EOF

# src/code_view.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/code_view.rs
use crate::document::{RenderedDocument, Span};
use crate::ansi::ColorMode;
use crate::theme::{Theme, SemanticColor};

pub fn render_code_file(
    content: &str,
    language: Option<&str>,
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
INNER_EOF

# src/binary.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/binary.rs
use crate::document::{RenderedDocument, Span};
use crate::theme::{Theme, SemanticColor};

pub fn render_binary_info(path: &std::path::Path, size: u64, _mtime: u64) -> RenderedDocument {
    let mut doc = RenderedDocument {
        lines: Vec::new(),
        headings: Vec::new(),
        links: Vec::new(),
    };
    let theme = Theme::Dark; // Default fallback
    doc.lines.push(vec![Span {
        text: format!("Binary file: {} ({} bytes)", path.display(), size),
        style: theme.style(SemanticColor::Foreground)
    }]);
    doc
}
INNER_EOF

# src/width.rs
cat << 'INNER_EOF' > crates/terminalwiki-render/src/width.rs
use unicode_width::UnicodeWidthStr;

pub fn display_width(s: &str) -> usize {
    s.width()
}

pub fn wrap_text(s: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    for word in s.split_whitespace() {
        if current_line.is_empty() {
            current_line.push_str(word);
        } else if display_width(&current_line) + 1 + display_width(word) <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    lines
}
INNER_EOF

chmod +x generate_render.sh
./generate_render.sh
