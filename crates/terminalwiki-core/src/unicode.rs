//! Centralized Unicode display and width measurement utilities (spec §67).
//!
//! Provides width calculation, truncation, and padding that respect grapheme
//! clusters, wide East Asian characters, combining marks, and emoji ZWJ sequences.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Returns the visual display width of a string in terminal columns.
pub fn display_width(s: &str) -> usize {
    s.width()
}

/// Truncates a string to fit within `max_width` terminal columns without breaking grapheme clusters.
/// Appends an ellipsis `…` if truncated and if space permits.
pub fn truncate_display_width(s: &str, max_width: usize) -> String {
    if s.width() <= max_width {
        return s.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let target_width = max_width - 1; // reserve 1 column for '…'
    let mut current_width = 0;
    let mut out = String::with_capacity(s.len());

    for grapheme in s.graphemes(true) {
        let gw = grapheme.width();
        if current_width + gw > target_width {
            break;
        }
        out.push_str(grapheme);
        current_width += gw;
    }
    out.push('…');
    out
}

/// Pads a string with spaces to reach exactly `target_width` terminal columns.
/// If the string is already wider, it is returned unmodified.
pub fn pad_display_width(s: &str, target_width: usize) -> String {
    let w = s.width();
    if w >= target_width {
        s.to_string()
    } else {
        let mut out = String::with_capacity(s.len() + (target_width - w));
        out.push_str(s);
        for _ in 0..(target_width - w) {
            out.push(' ');
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_ascii_and_unicode_widths() {
        assert_eq!(display_width("hello"), 5);
        assert_eq!(display_width("Größe"), 5);
        assert_eq!(display_width("漢字"), 4); // CJK characters are 2 columns each
        assert_eq!(display_width("🦀"), 2); // Emoji
    }

    #[test]
    fn truncates_without_breaking_graphemes() {
        assert_eq!(truncate_display_width("hello world", 6), "hello…");
        assert_eq!(truncate_display_width("漢字テスト", 5), "漢…");
        assert_eq!(truncate_display_width("short", 10), "short");
    }

    #[test]
    fn pads_correctly() {
        assert_eq!(pad_display_width("hi", 5), "hi   ");
        assert_eq!(display_width(&pad_display_width("漢字", 6)), 6);
    }
}
