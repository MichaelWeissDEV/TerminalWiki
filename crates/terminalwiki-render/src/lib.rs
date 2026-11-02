//! TerminalWiki Rendering Engine (spec §55, §56).

pub mod ansi;
pub mod binary;
pub mod code_view;
pub mod diagram;
pub mod document;
pub mod highlight;
pub mod image_view;
pub mod latex;
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
pub use image_view::{render_image_info, render_image_with_mode};
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

/// Main entry point for rendering a markdown page.
///
/// A leading YAML frontmatter block is metadata, not content, so it is stripped
/// before parsing — otherwise the `---` fences parse as a setext heading and the
/// raw keys are displayed to the user. The body boundary is taken from
/// [`terminalwiki_core::frontmatter::Frontmatter::body_offset`], the same source
/// the indexer uses, so display and
/// index always agree on where the body starts.
pub fn render_markdown(
    text: &str,
    config: &terminalwiki_core::config::Config,
    theme: &Theme,
    color_mode: ColorMode,
) -> RenderedDocument {
    let fm = terminalwiki_core::frontmatter::Frontmatter::parse(text);
    let body = text.get(fm.body_offset..).unwrap_or(text);
    render_markdown_raw(body, config, theme, color_mode)
}

/// Renders markdown text verbatim, **without** frontmatter stripping.
///
/// Used for plain-text files and for synthetic documents built in memory. A
/// `.txt` file is not a wiki page: the indexer does not parse frontmatter for
/// [`ContentType::Text`], so stripping it here would make display and index
/// disagree, and would silently hide everything between a leading `---` and the
/// next one.
pub fn render_markdown_raw(
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
        ContentType::Image => render_image_with_mode(path, bytes, theme, color_mode),
        ContentType::Code => {
            let text = String::from_utf8_lossy(bytes);
            let lang = terminalwiki_core::filetype::language_for(path);
            render_code_file(&text, lang, path, config, theme, color_mode, None)
        }
        ContentType::Latex => {
            let text = String::from_utf8_lossy(bytes);
            let is_tex_doc = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("tex") || e.eq_ignore_ascii_case("latex"));
            if is_tex_doc {
                latex::render_latex(Some(path), &text, config, theme, color_mode)
            } else {
                render_code_file(&text, Some("LaTeX"), path, config, theme, color_mode, None)
            }
        }
        ContentType::Markdown => {
            let text = String::from_utf8_lossy(bytes);
            let fm = terminalwiki_core::frontmatter::Frontmatter::parse(&text);
            let body = text.get(fm.body_offset..).unwrap_or(&text);
            let doc = markdown::parse_markdown(body);
            let renderer =
                render::Renderer::new(config.render.max_content_width, theme, color_mode)
                    .with_base_dir(path.parent());
            renderer.render_document(&doc)
        }
        // Plain text is rendered with the markdown renderer for convenience, but
        // it is not a wiki page — a leading `---` is content, not frontmatter.
        ContentType::Text => {
            let text = String::from_utf8_lossy(bytes);
            let doc = markdown::parse_markdown(&text);
            let renderer =
                render::Renderer::new(config.render.max_content_width, theme, color_mode)
                    .with_base_dir(path.parent());
            renderer.render_document(&doc)
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

#[cfg(test)]
mod frontmatter_render_tests {
    use super::*;
    use terminalwiki_core::config::Config;

    fn render_to_text(src: &str) -> String {
        let doc = render_markdown(src, &Config::default(), &Theme::Mono, ColorMode::Never);
        doc.lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Frontmatter is metadata and must never be displayed as page content.
    #[test]
    fn frontmatter_is_not_rendered_as_content() {
        let out = render_to_text("---\ntitle: Heap\ntags: [security]\n---\n# Heap\n\nBody text.\n");
        assert!(
            !out.contains("title:"),
            "frontmatter key leaked into output:\n{out}"
        );
        assert!(
            !out.contains("security"),
            "frontmatter tags leaked into output:\n{out}"
        );
        assert!(out.contains("Heap"), "heading missing from output:\n{out}");
        assert!(out.contains("Body text."), "body missing:\n{out}");
    }

    /// A document without frontmatter must be rendered in full.
    #[test]
    fn document_without_frontmatter_is_unchanged() {
        let out = render_to_text("# Title\n\nFirst paragraph.\n");
        assert!(out.contains("Title"));
        assert!(out.contains("First paragraph."));
    }

    /// A horizontal rule mid-document is content, not frontmatter.
    #[test]
    fn mid_document_rule_is_preserved() {
        let out = render_to_text("# Title\n\nBefore.\n\n---\n\nAfter.\n");
        assert!(out.contains("Before."), "content before rule lost:\n{out}");
        assert!(out.contains("After."), "content after rule lost:\n{out}");
    }

    #[test]
    fn rendered_links_have_precise_unicode_cell_regions() {
        let doc = render_markdown_raw(
            "前 [[One]] then [[Two]]",
            &Config::default(),
            &Theme::Mono,
            ColorMode::Never,
        );
        assert_eq!(doc.links.len(), 2);
        assert_eq!(
            (doc.links[0].start_column, doc.links[0].end_column),
            (3, 10)
        );
        assert_eq!(
            (doc.links[1].start_column, doc.links[1].end_column),
            (16, 23)
        );
    }

    #[test]
    fn highlights_and_task_glyphs_are_rendered() {
        let doc = render_markdown_raw(
            "- [x] done\n\nA ==marked== word.\n",
            &Config::default(),
            &Theme::Mono,
            ColorMode::Never,
        );
        let plain = doc
            .lines
            .iter()
            .map(|line| {
                line.iter()
                    .map(|span| span.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(plain.contains("☑ done"), "task glyph missing:\n{plain}");
        assert!(plain.contains("marked"), "highlight text missing:\n{plain}");
        assert!(!plain.contains("=="), "highlight markers leaked:\n{plain}");
    }

    #[test]
    fn wrapping_preserves_inline_styles() {
        let mut config = Config::default();
        config.render.max_content_width = 20;
        let doc = render_markdown_raw(
            "start **bold words that wrap** end",
            &config,
            &Theme::Mono,
            ColorMode::Never,
        );
        let styled: Vec<&str> = doc
            .lines
            .iter()
            .flat_map(|line| line.iter())
            .filter(|span| span.style.bold)
            .map(|span| span.text.as_str())
            .collect();
        assert!(
            styled.iter().any(|text| text.contains("bold")),
            "{styled:?}"
        );
        assert!(
            styled.iter().any(|text| text.contains("wrap")),
            "bold style lost after wrapping: {styled:?}"
        );
    }

    #[test]
    fn wrapped_link_hitbox_points_at_its_visual_line() {
        let mut config = Config::default();
        config.render.max_content_width = 20;
        let doc = render_markdown_raw(
            "one two three four five six seven [[Target]] tail",
            &config,
            &Theme::Mono,
            ColorMode::Never,
        );
        assert_eq!(doc.links.len(), 1);
        let link = &doc.links[0];
        assert!(link.line > 0, "link should be on a wrapped line: {link:?}");
        let plain: String = doc.lines[link.line]
            .iter()
            .map(|span| span.text.as_str())
            .collect();
        let cells: String = plain
            .chars()
            .skip(link.start_column)
            .take(link.end_column - link.start_column)
            .collect();
        assert_eq!(cells, "[[Target]]", "wrong hitbox in line {plain:?}");
    }

    #[test]
    fn body_tags_become_clickable_links() {
        let doc = render_markdown_raw(
            "See #rust and #project/web.\n",
            &Config::default(),
            &Theme::Mono,
            ColorMode::Never,
        );
        let targets: Vec<&LinkTarget> = doc.links.iter().map(|link| &link.target).collect();
        assert_eq!(
            targets,
            [
                &LinkTarget::Tag("rust".into()),
                &LinkTarget::Tag("project/web".into())
            ]
        );
        let plain: String = doc.lines[0].iter().map(|span| span.text.as_str()).collect();
        assert!(plain.contains("#rust"), "{plain}");
        let first = &doc.links[0];
        let cells: String = plain
            .chars()
            .skip(first.start_column)
            .take(first.end_column - first.start_column)
            .collect();
        assert_eq!(cells, "#rust");
    }

    #[test]
    fn duplicate_headings_receive_unique_suffix_anchors() {
        let doc = render_markdown_raw(
            "# Setup\n\n## Setup\n\n## Setup\n",
            &Config::default(),
            &Theme::Mono,
            ColorMode::Never,
        );
        let anchors: Vec<&str> = doc.headings.iter().map(|h| h.anchor.as_str()).collect();
        assert_eq!(anchors, ["setup", "setup-1", "setup-2"]);
    }

    #[test]
    fn heading_anchor_ignores_punctuation_and_case() {
        let doc = render_markdown_raw(
            "## Target Heading!\n",
            &Config::default(),
            &Theme::Mono,
            ColorMode::Never,
        );
        assert_eq!(doc.headings[0].anchor, "target-heading");
    }

    #[test]
    fn footnote_definitions_are_visible_in_terminal_output() {
        let out = render_to_text("Text[^n].\n\n[^n]: Footnote body.\n");
        assert!(out.contains("[^n]"), "footnote marker missing:\n{out}");
        assert!(
            out.contains("Footnote body."),
            "footnote body missing:\n{out}"
        );
    }

    #[test]
    fn note_embeds_are_clickable_navigation_fallbacks() {
        let doc = render_markdown_raw(
            "![[Guide#Install]]",
            &Config::default(),
            &Theme::Mono,
            ColorMode::Never,
        );
        assert_eq!(doc.links.len(), 1);
        assert!(matches!(
            &doc.links[0].target,
            LinkTarget::Wiki { page, anchor, .. }
                if page == "Guide" && anchor.as_deref() == Some("Install")
        ));
    }
}

#[cfg(test)]
mod text_vs_markdown_tests {
    use super::*;
    use std::path::Path;
    use terminalwiki_core::config::Config;

    fn lines_of(doc: &RenderedDocument) -> String {
        doc.lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// A `.txt` file is not a wiki page: a leading `---` block is content, and
    /// stripping it would hide text the indexer still indexes.
    #[test]
    fn text_files_keep_leading_dashed_block() {
        let src = "---\nnot: frontmatter\n---\nvisible body\n";
        let doc = render_path(
            Path::new("notes.txt"),
            src.as_bytes(),
            &Config::default(),
            &Theme::Mono,
            ColorMode::Never,
        );
        let out = lines_of(&doc);
        assert!(
            out.contains("not:") || out.contains("frontmatter"),
            "text file lost its leading block:\n{out}"
        );
        assert!(out.contains("visible body"), "text file lost body:\n{out}");
    }

    /// The same bytes as a `.md` page are frontmatter and must be stripped.
    #[test]
    fn markdown_files_strip_the_same_block() {
        let src = "---\ntitle: Real\n---\nvisible body\n";
        let doc = render_path(
            Path::new("page.md"),
            src.as_bytes(),
            &Config::default(),
            &Theme::Mono,
            ColorMode::Never,
        );
        let out = lines_of(&doc);
        assert!(!out.contains("title:"), "frontmatter leaked:\n{out}");
        assert!(out.contains("visible body"), "body lost:\n{out}");
    }
}
