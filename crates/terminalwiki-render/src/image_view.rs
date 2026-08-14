//! Terminal image information and metadata rendering (spec §5, §55).

use std::path::Path;

use crate::ansi::Style;
use crate::document::{RenderedDocument, Span};
use crate::theme::{SemanticColor, Theme};

pub struct ImageInfo {
    pub format: &'static str,
    pub dimensions: Option<(u32, u32)>,
    pub size: usize,
}

/// Sniffs image metadata (dimensions and image format) from raw bytes.
pub fn sniff_image_metadata(bytes: &[u8]) -> ImageInfo {
    let size = bytes.len();

    // PNG: \x89PNG\r\n\x1a\n
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return ImageInfo {
            format: "PNG",
            dimensions: Some((width, height)),
            size,
        };
    }

    // GIF: GIF87a or GIF89a
    if (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) && bytes.len() >= 10 {
        let width = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let height = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        return ImageInfo {
            format: "GIF",
            dimensions: Some((width, height)),
            size,
        };
    }

    // JPEG: 0xFF 0xD8
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] == 0xFF {
                let marker = bytes[i + 1];
                // SOF0 (0xC0) or SOF2 (0xC2)
                if marker == 0xC0 || marker == 0xC2 {
                    let height = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                    let width = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                    return ImageInfo {
                        format: "JPEG",
                        dimensions: Some((width, height)),
                        size,
                    };
                }
                // Skip segment
                if i + 3 < bytes.len() {
                    let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
                    i += 2 + len;
                    continue;
                }
            }
            i += 1;
        }
        return ImageInfo {
            format: "JPEG",
            dimensions: None,
            size,
        };
    }

    // WebP: RIFF....WEBP
    if bytes.starts_with(b"RIFF") && bytes.len() >= 30 && &bytes[8..12] == b"WEBP" {
        return ImageInfo {
            format: "WebP",
            dimensions: None,
            size,
        };
    }

    // SVG: <?xml or <svg
    if bytes.starts_with(b"<?xml") || bytes.starts_with(b"<svg") {
        return ImageInfo {
            format: "SVG",
            dimensions: None,
            size,
        };
    }

    ImageInfo {
        format: "Image",
        dimensions: None,
        size,
    }
}

pub fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Renders image metadata card without corrupting terminal or parsing raw binary.
pub fn render_image_info(path: &Path, bytes: &[u8], theme: &Theme) -> RenderedDocument {
    let mut doc = RenderedDocument::default();
    let meta = sniff_image_metadata(bytes);
    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("image");

    let header_style = theme.style(SemanticColor::Foreground).bold();
    let label_style = theme.style(SemanticColor::Muted);
    let value_style = theme.style(SemanticColor::Foreground);

    doc.lines.push(vec![Span {
        text: filename.to_string(),
        style: header_style,
    }]);
    doc.lines.push(vec![Span {
        text: String::new(),
        style: Style::new(),
    }]);

    doc.lines.push(vec![
        Span {
            text: "Type        ".to_string(),
            style: label_style.clone(),
        },
        Span {
            text: meta.format.to_string(),
            style: value_style.clone(),
        },
    ]);

    if let Some((w, h)) = meta.dimensions {
        doc.lines.push(vec![
            Span {
                text: "Dimensions  ".to_string(),
                style: label_style.clone(),
            },
            Span {
                text: format!("{w} × {h}"),
                style: value_style.clone(),
            },
        ]);
    }

    doc.lines.push(vec![
        Span {
            text: "Size        ".to_string(),
            style: label_style,
        },
        Span {
            text: format_size(meta.size),
            style: value_style,
        },
    ]);

    doc
}
