//! Terminal escape sanitization (spec §43, §97, §98).
//!
//! Wiki content is untrusted. A markdown file, a code file, a file *name*, a
//! tag or a link title may contain raw ANSI/OSC/DCS byte sequences. None of
//! them may ever reach the terminal: only escape sequences that TerminalWiki
//! itself generates are allowed out.
//!
//! # Why this strips introducers instead of parsing sequences
//!
//! A sanitizer that tries to *recognise* `CSI ... m`, `OSC 52 ... BEL`, `DCS
//! ... ST` and friends has to model every terminal's parser correctly, and any
//! gap in that model is an injection bug. This module takes the inverse,
//! non-negotiable approach: it removes the characters that can *introduce* a
//! control sequence at all.
//!
//! * `ESC` (0x1B) — introduces CSI/OSC/DCS/APC/PM/SS2/SS3 and every 7-bit form.
//! * All other C0 controls (0x00–0x1F), including `BEL`, `SO`/`SI` and `CR`.
//! * `DEL` (0x7F).
//! * The C1 block (U+0080–U+009F) — the 8-bit forms of CSI, OSC, DCS, APC, PM.
//! * Unicode bidirectional overrides and isolates (the "Trojan Source" class),
//!   which can visually reorder text without changing its bytes.
//!
//! With every introducer gone, the remainder of a would-be sequence is inert
//! text. `ESC ] 52 ; c ; <base64> BEL` becomes the harmless literal
//! `]52;c;<base64>`. There is no ordering, no state machine and no lookahead,
//! which is exactly what makes the property fuzzable and auditable.
//!
//! Newlines are a control character too, and are only preserved where the
//! caller explicitly asks for multi-line text.

use unicode_width::UnicodeWidthChar;

/// How removed control characters are represented in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Policy {
    /// Drop control characters entirely. The safe default.
    #[default]
    Strip,
    /// Replace control characters with a printable glyph from the Unicode
    /// "Control Pictures" block, so the reader can see that something was
    /// there. Useful when viewing code files that legitimately contain them.
    Visible,
}

/// Default tab stop width used when expanding tabs.
pub const DEFAULT_TAB_WIDTH: usize = 4;

/// Returns true if `c` must never be forwarded to the terminal.
///
/// Note that `\n` and `\t` are included here: they are handled separately and
/// deliberately by the sanitizing functions, never passed through implicitly.
#[inline]
pub fn is_forbidden(c: char) -> bool {
    matches!(c,
        // C0 controls and DEL.
        '\u{0}'..='\u{1F}' | '\u{7F}'
        // C1 controls: the 8-bit CSI/OSC/DCS/APC/PM introducers.
        | '\u{80}'..='\u{9F}'
        // Bidi overrides / embeddings / isolates ("Trojan Source").
        | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
        // Zero-width joiner control usage and the BOM as an interior char.
        | '\u{FEFF}'
    )
}

/// The printable stand-in for a control character under [`Policy::Visible`].
fn visible_glyph(c: char) -> char {
    match c {
        // Control Pictures block mirrors C0 one-to-one at U+2400.
        '\u{0}'..='\u{1F}' => char::from_u32(0x2400 + c as u32).unwrap_or('\u{FFFD}'),
        '\u{7F}' => '\u{2421}',
        _ => '\u{FFFD}',
    }
}

/// Sanitize a single line of untrusted text.
///
/// Newlines are treated as control characters and removed, so the result is
/// guaranteed to occupy exactly one terminal line. Use this for file names,
/// tags, link titles, headings in lists, and search-result snippets.
pub fn sanitize_line(input: &str) -> String {
    sanitize_line_with(input, Policy::Strip, DEFAULT_TAB_WIDTH)
}

/// [`sanitize_line`] with an explicit policy and tab width.
pub fn sanitize_line_with(input: &str, policy: Policy, tab_width: usize) -> String {
    let mut out = String::with_capacity(input.len());
    let mut column = 0usize;
    for c in input.chars() {
        push_char(&mut out, &mut column, c, policy, tab_width, false);
    }
    out
}

/// Sanitize multi-line untrusted text, preserving `\n` only.
///
/// `\r` is removed, so CRLF input collapses to LF and a lone `\r` can never be
/// used to overwrite a line that was already drawn.
pub fn sanitize_text(input: &str) -> String {
    sanitize_text_with(input, Policy::Strip, DEFAULT_TAB_WIDTH)
}

/// [`sanitize_text`] with an explicit policy and tab width.
pub fn sanitize_text_with(input: &str, policy: Policy, tab_width: usize) -> String {
    let mut out = String::with_capacity(input.len());
    let mut column = 0usize;
    for c in input.chars() {
        if c == '\n' {
            out.push('\n');
            column = 0;
            continue;
        }
        push_char(&mut out, &mut column, c, policy, tab_width, true);
    }
    out
}

#[inline]
fn push_char(
    out: &mut String,
    column: &mut usize,
    c: char,
    policy: Policy,
    tab_width: usize,
    _multiline: bool,
) {
    if c == '\t' {
        // Expand to the next tab stop so code stays aligned without ever
        // emitting a raw tab (terminals disagree about tab stops).
        let width = tab_width.max(1);
        let advance = width - (*column % width);
        for _ in 0..advance {
            out.push(' ');
        }
        *column += advance;
        return;
    }
    if is_forbidden(c) {
        if policy == Policy::Visible {
            let g = visible_glyph(c);
            out.push(g);
            *column += g.width().unwrap_or(0);
        }
        return;
    }
    out.push(c);
    *column += c.width().unwrap_or(0);
}

/// Sanitize bytes that are not guaranteed to be valid UTF-8 (spec §67, §98).
///
/// Invalid sequences become U+FFFD rather than causing an error, so a corrupt
/// file is still viewable and can never smuggle bytes through.
pub fn sanitize_bytes(input: &[u8]) -> String {
    sanitize_text(&String::from_utf8_lossy(input))
}

/// Sanitize a single line supplied as bytes.
pub fn sanitize_line_bytes(input: &[u8]) -> String {
    sanitize_line(&String::from_utf8_lossy(input))
}

/// Sanitize a path for display, tolerating non-UTF-8 components (spec §67).
pub fn sanitize_path_display(path: &std::path::Path) -> String {
    sanitize_line(&path.to_string_lossy())
}

/// Validate a URL before it may be used in an OSC-8 hyperlink (spec §44).
///
/// Returns `None` when the URL must not be turned into a terminal hyperlink.
/// The check is deliberately conservative: only a small scheme allowlist, no
/// control characters at all, and a bounded length.
pub fn safe_hyperlink(url: &str) -> Option<String> {
    const MAX_URL: usize = 2048;
    if url.is_empty() || url.len() > MAX_URL {
        return None;
    }
    // Any control character disqualifies the URL outright; we do not try to
    // repair it, because a repaired URL is not the one the author wrote.
    if url.chars().any(is_forbidden) || url.chars().any(|c| c.is_whitespace()) {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    const ALLOWED: [&str; 5] = ["http://", "https://", "mailto:", "ftp://", "file://"];
    if ALLOWED.iter().any(|s| lower.starts_with(s)) {
        Some(url.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_osc52_clipboard_sequence() {
        // The classic clipboard-hijack payload.
        let evil = "before\x1b]52;c;ZWNobyBwd25lZAo=\x07after";
        let clean = sanitize_text(evil);
        assert!(!clean.contains('\x1b'));
        assert!(!clean.contains('\x07'));
        assert_eq!(clean, "before]52;c;ZWNobyBwd25lZAo=after");
    }

    #[test]
    fn strips_osc8_hyperlink_smuggling() {
        let evil = "\x1b]8;;http://evil.example\x1b\\click\x1b]8;;\x1b\\";
        let clean = sanitize_text(evil);
        assert!(!clean.contains('\x1b'));
        assert!(clean.contains("click"));
    }

    #[test]
    fn strips_csi_sequences() {
        let evil = "red\x1b[31mtext\x1b[0m";
        assert_eq!(sanitize_text(evil), "red[31mtext[0m");
    }

    #[test]
    fn strips_dcs_apc_and_pm() {
        for intro in ["\x1bP", "\x1b_", "\x1b^"] {
            let evil = format!("a{intro}payload\x1b\\b");
            let clean = sanitize_text(&evil);
            assert!(!clean.contains('\x1b'), "escape survived in {clean:?}");
        }
    }

    #[test]
    fn strips_eight_bit_c1_introducers() {
        // U+009B is the 8-bit CSI, U+009D the 8-bit OSC.
        let evil = "a\u{9b}31mb\u{9d}52;c;xx";
        let clean = sanitize_text(evil);
        assert!(!clean.contains('\u{9b}'));
        assert!(!clean.contains('\u{9d}'));
    }

    #[test]
    fn strips_bidi_trojan_source_characters() {
        let evil = "if (admin) {\u{202E} // \u{202D}";
        let clean = sanitize_text(evil);
        assert!(!clean.contains('\u{202E}'));
        assert!(!clean.contains('\u{202D}'));
    }

    #[test]
    fn carriage_return_cannot_overwrite_a_drawn_line() {
        assert_eq!(sanitize_text("safe\rEVIL"), "safeEVIL");
    }

    #[test]
    fn line_mode_never_yields_more_than_one_line() {
        let evil = "name\nrm -rf /\nmore";
        let clean = sanitize_line(evil);
        assert!(!clean.contains('\n'));
    }

    #[test]
    fn text_mode_preserves_newlines_only() {
        assert_eq!(sanitize_text("a\nb\r\nc"), "a\nb\nc");
    }

    #[test]
    fn tabs_expand_to_tab_stops() {
        assert_eq!(sanitize_text_with("a\tb", Policy::Strip, 4), "a   b");
        assert_eq!(sanitize_text_with("\tx", Policy::Strip, 4), "    x");
        assert_eq!(sanitize_text_with("abcd\te", Policy::Strip, 4), "abcd    e");
    }

    #[test]
    fn tab_stops_reset_on_each_line() {
        assert_eq!(sanitize_text_with("ab\n\tc", Policy::Strip, 4), "ab\n    c");
    }

    #[test]
    fn visible_policy_uses_printable_control_pictures() {
        let clean = sanitize_text_with("a\x1bb", Policy::Visible, 4);
        assert_eq!(clean, "a\u{241B}b");
        assert!(!clean.contains('\x1b'));
    }

    #[test]
    fn invalid_utf8_becomes_replacement_characters() {
        let clean = sanitize_bytes(&[b'a', 0xFF, 0xFE, b'b', 0x1b, b'[', b'3', b'1', b'm']);
        assert!(!clean.contains('\x1b'));
        assert!(clean.starts_with('a'));
    }

    #[test]
    fn no_control_character_survives_any_input() {
        // Exhaustive over the whole C0/C1 range plus DEL.
        for cp in (0u32..=0x9F).chain(std::iter::once(0x7Fu32)) {
            let c = char::from_u32(cp).unwrap();
            let s = format!("x{c}y");
            let clean = sanitize_text(&s);
            for out in clean.chars() {
                if out == '\n' {
                    continue; // explicitly allowed in text mode
                }
                assert!(!is_forbidden(out), "cp {cp:#x} produced forbidden char {out:?}");
            }
        }
    }

    #[test]
    fn very_long_lines_are_handled_without_panic() {
        let long = "\x1b[31m".repeat(200_000);
        let clean = sanitize_text(&long);
        assert!(!clean.contains('\x1b'));
    }

    #[test]
    fn hyperlink_validation_rejects_dangerous_urls() {
        assert!(safe_hyperlink("https://kernel.org").is_some());
        assert!(safe_hyperlink("http://example.com/a?b=c").is_some());
        assert!(safe_hyperlink("mailto:a@b.c").is_some());
        // Rejected: unknown scheme, control chars, whitespace, oversize.
        assert!(safe_hyperlink("javascript:alert(1)").is_none());
        assert!(safe_hyperlink("https://a.b/\x1b]52;c;x").is_none());
        assert!(safe_hyperlink("https://a.b/ x").is_none());
        assert!(safe_hyperlink("").is_none());
        assert!(safe_hyperlink(&format!("https://a.b/{}", "x".repeat(4000))).is_none());
    }

    #[test]
    fn sanitizing_is_idempotent() {
        let evil = "a\x1b]8;;u\x07b\u{202E}c\td";
        let once = sanitize_text(evil);
        assert_eq!(sanitize_text(&once), once);
    }
}
