//! TerminalWiki Rendering Engine (spec §55, §56).

pub mod ansi;
pub mod binary;
pub mod code_view;
pub mod document;
pub mod highlight;
pub mod image_view;
pub mod markdown;
pub mod math;
pub mod plain;
pub mod render;
pub mod theme;
pub mod width;

pub use ansi::{Color, ColorMode, Style};
pub use binary::render_binary_info;
pub use code_view::{render_code_file, render_code_with_options, CodeRenderOptions};
pub use document::{
    Block, Document, Inline, LinkTarget, RenderedDocument, RenderedHeading, RenderedLine,
    RenderedLink, Span,
};
pub use image_view::render_image_info;
pub use markdown::parse_markdown;
pub use math::MathRenderer;
pub use plain::PlainRenderer;
pub use render::Renderer;
pub use theme::{SemanticColor, Theme};

use std::io::Write;
use std::path::Path;
use terminalwiki_core::filetype::{classify, ContentType};

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

/// Centralized content-type dispatch rendering any file into a `RenderedDocument` (spec §27, §55).
pub fn render_path(
    path: &Path,
    bytes: &[u8],
    config: &terminalwiki_core::config::Config,
    theme: &Theme,
    color_mode: ColorMode,
) -> RenderedDocument {
    let content_type = classify(path, bytes);

    match content_type {
        ContentType::Binary => {
            let len = bytes.len() as u64;
            let mtime = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .map(|t| {
                    t.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                })
                .unwrap_or(0);
            render_binary_info(path, len, mtime)
        }
        ContentType::Image => render_image_info(path, bytes, theme),
        ContentType::Code | ContentType::Latex => {
            let text = String::from_utf8_lossy(bytes);
            let lang = terminalwiki_core::filetype::language_for(path);
            render_code_file(&text, lang, path, config, theme, color_mode, None)
        }
        ContentType::Markdown | ContentType::Text => {
            let text = String::from_utf8_lossy(bytes);
            render_markdown(&text, config, theme, color_mode)
        }
    }
}

/// Write rendered document to a Writer (stdout, stderr, buffer)
pub fn write_document(
    doc: &RenderedDocument,
    out: &mut impl Write,
    color: ColorMode,
) -> std::io::Result<()> {
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
