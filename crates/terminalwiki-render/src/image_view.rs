//! Terminal image decoding, metadata inspection, and Unicode half-block rendering (spec §5, §55).

use std::io::Read;
use std::path::Path;

use flate2::read::ZlibDecoder;

use crate::ansi::{Color, ColorMode, Style};
use crate::document::{RenderedDocument, RenderedLine, Span};
use crate::theme::{SemanticColor, Theme};

pub struct ImageInfo {
    pub format: &'static str,
    pub dimensions: Option<(u32, u32)>,
    pub size: usize,
}

/// Decoded RGB image buffer (3 bytes per pixel: R, G, B).
#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

const MAX_DECODE_PIXELS: usize = 4096 * 4096;

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

    // BMP: BM
    if bytes.starts_with(b"BM") && bytes.len() >= 26 {
        let width = i32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]).unsigned_abs();
        let height =
            i32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]).unsigned_abs();
        return ImageInfo {
            format: "BMP",
            dimensions: Some((width, height)),
            size,
        };
    }

    // PPM / PGM: P6 or P5
    if (bytes.starts_with(b"P6") || bytes.starts_with(b"P5") || bytes.starts_with(b"P3"))
        && bytes.len() >= 7
    {
        if let Some((w, h, _, _)) = parse_pnm_header(bytes) {
            return ImageInfo {
                format: "PNM",
                dimensions: Some((w, h)),
                size,
            };
        }
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

/// Decodes supported image formats (PNG, BMP, PPM/PGM, simple SVG) into an RGB buffer.
pub fn decode_image(bytes: &[u8]) -> Option<DecodedImage> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return decode_png(bytes);
    }
    if bytes.starts_with(b"BM") {
        return decode_bmp(bytes);
    }
    if bytes.starts_with(b"P6") || bytes.starts_with(b"P5") || bytes.starts_with(b"P3") {
        return decode_pnm(bytes);
    }
    if bytes.starts_with(b"<?xml") || bytes.starts_with(b"<svg") {
        return decode_svg_preview(bytes);
    }
    None
}

fn decode_png(bytes: &[u8]) -> Option<DecodedImage> {
    if bytes.len() < 33 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let mut pos = 8;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut bit_depth = 0u8;
    let mut color_type = 0u8;
    let mut interlace = 0u8;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut idat = Vec::new();

    while pos + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
            as usize;
        let chunk_type = &bytes[pos + 4..pos + 8];
        pos += 8;
        if pos + len + 4 > bytes.len() {
            break;
        }
        let data = &bytes[pos..pos + len];
        pos += len + 4; // skip CRC

        match chunk_type {
            b"IHDR" if data.len() >= 13 => {
                width = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                height = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
                bit_depth = data[8];
                color_type = data[9];
                interlace = data[12];
            }
            b"PLTE" => {
                for chunk in data.chunks_exact(3) {
                    palette.push([chunk[0], chunk[1], chunk[2]]);
                }
            }
            b"IDAT" => {
                idat.extend_from_slice(data);
            }
            b"IEND" => break,
            _ => {}
        }
    }

    if width == 0
        || height == 0
        || bit_depth != 8
        || interlace != 0
        || (width as usize).saturating_mul(height as usize) > MAX_DECODE_PIXELS
    {
        return None;
    }

    let bpp = match color_type {
        0 => 1usize, // Grayscale
        2 => 3usize, // RGB
        3 => 1usize, // Indexed palette
        4 => 2usize, // Grayscale + Alpha
        6 => 4usize, // RGBA
        _ => return None,
    };

    let stride = (width as usize) * bpp;
    let expected_raw = (height as usize) * (stride + 1);
    let mut raw = Vec::with_capacity(expected_raw);
    let mut decoder = ZlibDecoder::new(&idat[..]);
    decoder.read_to_end(&mut raw).ok()?;
    if raw.len() < expected_raw {
        return None;
    }

    let mut prev_row = vec![0u8; stride];
    let mut cur_row = vec![0u8; stride];
    let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);

    for y in 0..(height as usize) {
        let row_start = y * (stride + 1);
        let filter = raw[row_start];
        let src = &raw[row_start + 1..row_start + 1 + stride];

        for i in 0..stride {
            let a = if i >= bpp { cur_row[i - bpp] } else { 0 };
            let b = prev_row[i];
            let c = if i >= bpp { prev_row[i - bpp] } else { 0 };
            cur_row[i] = match filter {
                0 => src[i],
                1 => src[i].wrapping_add(a),
                2 => src[i].wrapping_add(b),
                3 => src[i].wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => src[i].wrapping_add(paeth_predictor(a, b, c)),
                _ => src[i],
            };
        }

        for px in cur_row.chunks_exact(bpp) {
            match color_type {
                0 => rgb.extend_from_slice(&[px[0], px[0], px[0]]),
                2 => rgb.extend_from_slice(&[px[0], px[1], px[2]]),
                3 => {
                    let c = palette.get(px[0] as usize).copied().unwrap_or([0, 0, 0]);
                    rgb.extend_from_slice(&c);
                }
                4 => {
                    let alpha = px[1] as u16;
                    let v = ((px[0] as u16 * alpha + 30 * (255 - alpha)) / 255) as u8;
                    rgb.extend_from_slice(&[v, v, v]);
                }
                6 => {
                    let alpha = px[3] as u16;
                    let r = ((px[0] as u16 * alpha + 30 * (255 - alpha)) / 255) as u8;
                    let g = ((px[1] as u16 * alpha + 30 * (255 - alpha)) / 255) as u8;
                    let b = ((px[2] as u16 * alpha + 30 * (255 - alpha)) / 255) as u8;
                    rgb.extend_from_slice(&[r, g, b]);
                }
                _ => {}
            }
        }

        prev_row.copy_from_slice(&cur_row);
    }

    Some(DecodedImage { width, height, rgb })
}

fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let a = a as i32;
    let b = b as i32;
    let c = c as i32;
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

fn decode_bmp(bytes: &[u8]) -> Option<DecodedImage> {
    if bytes.len() < 54 || !bytes.starts_with(b"BM") {
        return None;
    }
    let data_offset = u32::from_le_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]) as usize;
    let width = i32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]);
    let height_raw = i32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]);
    let bpp = u16::from_le_bytes([bytes[28], bytes[29]]);
    let compression = u32::from_le_bytes([bytes[30], bytes[31], bytes[32], bytes[33]]);
    if width <= 0 || height_raw == 0 || compression != 0 || (bpp != 24 && bpp != 32) {
        return None;
    }
    let w = width as u32;
    let h = height_raw.unsigned_abs();
    if (w as usize).saturating_mul(h as usize) > MAX_DECODE_PIXELS {
        return None;
    }
    let top_down = height_raw < 0;
    let bytes_per_pixel = (bpp / 8) as usize;
    let row_stride = ((w as usize) * bytes_per_pixel).div_ceil(4) * 4;
    if data_offset + row_stride * (h as usize) > bytes.len() {
        return None;
    }
    let mut rgb = vec![0u8; (w as usize) * (h as usize) * 3];
    for y in 0..(h as usize) {
        let src_y = if top_down { y } else { (h as usize) - 1 - y };
        let row = &bytes[data_offset + src_y * row_stride..];
        for x in 0..(w as usize) {
            let px = &row[x * bytes_per_pixel..];
            let dst = (y * (w as usize) + x) * 3;
            rgb[dst] = px[2];
            rgb[dst + 1] = px[1];
            rgb[dst + 2] = px[0];
        }
    }
    Some(DecodedImage {
        width: w,
        height: h,
        rgb,
    })
}

fn parse_pnm_header(bytes: &[u8]) -> Option<(u32, u32, u32, usize)> {
    let mut pos = 2;
    let mut tokens = Vec::new();
    while tokens.len() < 3 && pos < bytes.len() {
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos < bytes.len() && bytes[pos] == b'#' {
            while pos < bytes.len() && bytes[pos] != b'\n' {
                pos += 1;
            }
            continue;
        }
        let start = pos;
        while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() && bytes[pos] != b'#' {
            pos += 1;
        }
        if start < pos {
            let val: u32 = std::str::from_utf8(&bytes[start..pos]).ok()?.parse().ok()?;
            tokens.push(val);
        }
    }
    if tokens.len() == 3 && pos < bytes.len() {
        pos += 1; // single whitespace byte after maxval
        Some((tokens[0], tokens[1], tokens[2], pos))
    } else {
        None
    }
}

fn decode_pnm(bytes: &[u8]) -> Option<DecodedImage> {
    let (w, h, _maxval, data_start) = parse_pnm_header(bytes)?;
    let count = (w as usize).checked_mul(h as usize)?;
    if count == 0 || count > MAX_DECODE_PIXELS {
        return None;
    }
    let data = &bytes[data_start..];
    if bytes.starts_with(b"P6") && data.len() >= count * 3 {
        Some(DecodedImage {
            width: w,
            height: h,
            rgb: data[..count * 3].to_vec(),
        })
    } else if bytes.starts_with(b"P5") && data.len() >= count {
        let mut rgb = Vec::with_capacity(count * 3);
        for &g in &data[..count] {
            rgb.extend_from_slice(&[g, g, g]);
        }
        Some(DecodedImage {
            width: w,
            height: h,
            rgb,
        })
    } else {
        None
    }
}

fn decode_svg_preview(bytes: &[u8]) -> Option<DecodedImage> {
    let text = std::str::from_utf8(bytes).ok()?;
    let w = 48u32;
    let h = 24u32;
    let mut rgb = vec![28u8; (w * h * 3) as usize];
    for y in 0..h {
        for x in 0..w {
            let idx = ((y * w + x) * 3) as usize;
            let border = x == 0 || y == 0 || x + 1 == w || y + 1 == h;
            if border {
                rgb[idx] = 80;
                rgb[idx + 1] = 160;
                rgb[idx + 2] = 220;
            } else if text.contains("<circle") {
                let dx = x as f32 - (w as f32 / 2.0);
                let dy = (y as f32 - (h as f32 / 2.0)) * 1.6;
                if dx * dx + dy * dy <= 95.0 {
                    rgb[idx] = 70;
                    rgb[idx + 1] = 190;
                    rgb[idx + 2] = 150;
                }
            } else if (4..w - 4).contains(&x) && (4..h - 4).contains(&y) {
                rgb[idx] = 60 + ((x * 3) % 140) as u8;
                rgb[idx + 1] = 110 + ((y * 4) % 110) as u8;
                rgb[idx + 2] = 190;
            }
        }
    }
    Some(DecodedImage {
        width: w,
        height: h,
        rgb,
    })
}

/// Renders a decoded RGB image as Unicode half-blocks (`▀`) or monochrome shading.
pub fn render_decoded_canvas(
    img: &DecodedImage,
    max_cols: usize,
    max_rows: usize,
    indent: &str,
    theme: &Theme,
    color_mode: ColorMode,
) -> Vec<RenderedLine> {
    if img.width == 0 || img.height == 0 {
        return Vec::new();
    }
    let max_cols = max_cols.clamp(8, 72);
    let max_rows = max_rows.clamp(4, 24);

    // Each terminal character cell has ~1:2 width-to-height aspect ratio, and
    // each half-block `▀` encodes 2 vertical pixels per cell.
    let target_w = (img.width as usize).min(max_cols).max(4);
    let scale = target_w as f64 / img.width as f64;
    let target_px_h = ((img.height as f64 * scale).round() as usize).clamp(2, max_rows * 2);
    let cell_rows = target_px_h.div_ceil(2);

    let mono = color_mode == ColorMode::Never || *theme == Theme::Mono;
    let border_style = theme.style(SemanticColor::Muted);
    let mut lines = Vec::with_capacity(cell_rows + 2);

    lines.push(vec![Span {
        text: format!("{indent}╭{}╮", "─".repeat(target_w)),
        style: border_style.clone(),
    }]);

    for cy in 0..cell_rows {
        let mut row_spans = Vec::with_capacity(target_w + 2);
        row_spans.push(Span {
            text: format!("{indent}│"),
            style: border_style.clone(),
        });

        let py_top = cy * 2;
        let py_bot = (cy * 2 + 1).min(target_px_h.saturating_sub(1));

        for cx in 0..target_w {
            let top = sample_pixel(img, cx, py_top, target_w, target_px_h);
            let bot = sample_pixel(img, cx, py_bot, target_w, target_px_h);

            if mono {
                let lum = ((top[0] as u32
                    + top[1] as u32
                    + top[2] as u32
                    + bot[0] as u32
                    + bot[1] as u32
                    + bot[2] as u32)
                    / 6) as u8;
                let glyph = match lum {
                    0..=40 => " ",
                    41..=95 => "░",
                    96..=150 => "▒",
                    151..=205 => "▓",
                    _ => "█",
                };
                row_spans.push(Span {
                    text: glyph.to_string(),
                    style: theme.style(SemanticColor::Foreground),
                });
            } else {
                let style = Style::new()
                    .fg(Color::Rgb(top[0], top[1], top[2]))
                    .bg(Color::Rgb(bot[0], bot[1], bot[2]));
                row_spans.push(Span {
                    text: "▀".to_string(),
                    style,
                });
            }
        }

        row_spans.push(Span {
            text: "│".to_string(),
            style: border_style.clone(),
        });
        lines.push(row_spans);
    }

    lines.push(vec![Span {
        text: format!("{indent}╰{}╯", "─".repeat(target_w)),
        style: border_style,
    }]);

    lines
}

fn sample_pixel(
    img: &DecodedImage,
    cx: usize,
    py: usize,
    target_w: usize,
    target_h: usize,
) -> [u8; 3] {
    let sx = ((cx * img.width as usize) / target_w.max(1)).min(img.width as usize - 1);
    let sy = ((py * img.height as usize) / target_h.max(1)).min(img.height as usize - 1);
    let idx = (sy * img.width as usize + sx) * 3;
    if idx + 2 < img.rgb.len() {
        [img.rgb[idx], img.rgb[idx + 1], img.rgb[idx + 2]]
    } else {
        [0, 0, 0]
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

/// Renders image metadata card + visual terminal half-block preview when decodable.
pub fn render_image_info(path: &Path, bytes: &[u8], theme: &Theme) -> RenderedDocument {
    render_image_with_mode(path, bytes, theme, ColorMode::Auto)
}

/// Renders an image file with explicit `ColorMode`.
pub fn render_image_with_mode(
    path: &Path,
    bytes: &[u8],
    theme: &Theme,
    color_mode: ColorMode,
) -> RenderedDocument {
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

    if let Some(decoded) = decode_image(bytes) {
        doc.lines.push(Vec::new());
        let canvas = render_decoded_canvas(&decoded, 56, 20, "", theme, color_mode);
        doc.lines.extend(canvas);
    }

    doc
}

/// Resolves and renders an inline Markdown image (`![alt](url)` or `![[image.png]]`)
/// when `base_dir` is known and the image file exists locally.
pub fn render_inline_image(
    base_dir: Option<&Path>,
    alt: &str,
    url: &str,
    indent: &str,
    max_width: usize,
    theme: &Theme,
    color_mode: ColorMode,
) -> Vec<RenderedLine> {
    let clean_url = url.split('#').next().unwrap_or(url).trim();
    if let Some(base) = base_dir {
        let candidate = base.join(clean_url);
        if let Ok(bytes) = std::fs::read(&candidate) {
            let meta = sniff_image_metadata(&bytes);
            let mut out = Vec::new();
            let dim_label = meta
                .dimensions
                .map(|(w, h)| format!(" · {w}×{h}"))
                .unwrap_or_default();
            let title = if alt.is_empty() { clean_url } else { alt };
            out.push(vec![Span {
                text: format!(
                    "{indent}🖼 {title} ({}{dim_label}, {})",
                    meta.format,
                    format_size(meta.size)
                ),
                style: theme.style(SemanticColor::Accent).bold(),
            }]);
            if let Some(decoded) = decode_image(&bytes) {
                out.extend(render_decoded_canvas(
                    &decoded,
                    max_width.saturating_sub(4).min(52),
                    14,
                    indent,
                    theme,
                    color_mode,
                ));
                return out;
            }
            return out;
        }
    }

    vec![vec![Span {
        text: format!("{indent}[image: {alt} ({url})]"),
        style: theme.style(SemanticColor::Muted).italic(),
    }]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_renders_pnm_image_to_half_blocks() {
        // 4x2 RGB P6 image
        let mut pnm = b"P6\n4 2\n255\n".to_vec();
        for _ in 0..8 {
            pnm.extend_from_slice(&[240, 100, 50]);
        }
        let decoded = decode_image(&pnm).expect("PNM should decode");
        assert_eq!((decoded.width, decoded.height), (4, 2));
        let lines = render_decoded_canvas(&decoded, 16, 8, "", &Theme::Dark, ColorMode::Always);
        assert!(lines.len() >= 3);
        let text: String = lines
            .iter()
            .flat_map(|l| l.iter().map(|s| s.text.as_str()))
            .collect();
        assert!(text.contains('▀'));
    }
}
