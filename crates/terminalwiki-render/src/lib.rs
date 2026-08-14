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
pub use code_view::{render_code_file, render_code_with_options, CodeRenderOptions};
pub use document::{Block, Document, Inline, RenderedDocument, RenderedLine, Span};
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
