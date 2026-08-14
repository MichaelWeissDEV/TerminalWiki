//! Wiki link syntax (spec §6, §28, §29).
//!
//! Every documented form is parsed into one structured [`WikiLink`]:
//!
//! ```text
//! [[Heap Exploitation]]                  page by name
//! [[Security/Heap]]                      page by path
//! [[Security/Heap|Heap Exploitation]]    page with display text
//! [[Page#Heading]]                       page anchor
//! [[rust::Ownership]]                    page in another wiki
//! [[file:src/main.rs]]                   code file
//! [[file:src/main.rs#L30]]               code file at a line
//! [[file:src/main.rs#L30-L55]]           code file, line range
//! [[symbol:tw::index::Indexer]]          symbol (resolution lands with tree-sitter)
//! ```
//!
//! Parsing the *syntax* is separate from resolving it, which is what lets the
//! symbol form be represented and linted today while its resolution arrives
//! later (spec §29) — without leaving a half-working feature in the UI
//! (spec §110).

/// A parsed line reference from a `file:` link (spec §28).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    /// First line, 1-based.
    pub start: u32,
    /// Last line, 1-based and inclusive. Equals `start` for a single line.
    pub end: u32,
}

impl LineRange {
    pub fn single(line: u32) -> Self {
        LineRange { start: line, end: line }
    }

    pub fn contains(&self, line: u32) -> bool {
        line >= self.start && line <= self.end
    }
}

/// What a wiki link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// A wiki page, optionally in another wiki, optionally at a heading.
    Page { wiki: Option<String>, name: String, anchor: Option<String> },
    /// A file addressed explicitly, optionally at a line or range.
    File { wiki: Option<String>, path: String, lines: Option<LineRange> },
    /// A symbol. Parsed and linted now; resolved once tree-sitter lands (spec §29).
    Symbol { name: String },
}

/// A parsed `[[...]]` link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiLink {
    pub target: LinkTarget,
    /// Explicit display text after `|`, if given.
    pub label: Option<String>,
    /// The exact source text between the brackets, for diagnostics.
    pub raw: String,
}

impl WikiLink {
    /// The text to display for this link.
    pub fn display_text(&self) -> String {
        if let Some(label) = &self.label {
            return label.clone();
        }
        match &self.target {
            LinkTarget::Page { name, anchor, .. } => match anchor {
                Some(a) if name.is_empty() => a.clone(),
                Some(a) => format!("{name} § {a}"),
                None => name.clone(),
            },
            LinkTarget::File { path, lines, .. } => match lines {
                Some(r) if r.start == r.end => format!("{path}:{}", r.start),
                Some(r) => format!("{path}:{}-{}", r.start, r.end),
                None => path.clone(),
            },
            LinkTarget::Symbol { name } => name.clone(),
        }
    }

    /// The wiki this link crosses into, if any (spec §6).
    pub fn wiki(&self) -> Option<&str> {
        match &self.target {
            LinkTarget::Page { wiki, .. } | LinkTarget::File { wiki, .. } => wiki.as_deref(),
            LinkTarget::Symbol { .. } => None,
        }
    }

    /// Parses the contents *between* `[[` and `]]`.
    ///
    /// Returns `None` only for an empty target, which is not a link at all.
    pub fn parse_inner(inner: &str) -> Option<WikiLink> {
        let raw = inner.to_string();

        // Split the display label first: everything after the first unescaped
        // `|` is display text and must not be interpreted as target syntax.
        let (target_part, label) = match inner.split_once('|') {
            Some((t, l)) => {
                let l = l.trim();
                (t.trim(), (!l.is_empty()).then(|| l.to_string()))
            }
            None => (inner.trim(), None),
        };

        if target_part.is_empty() {
            return None;
        }

        // `file:` prefix (spec §28).
        if let Some(rest) = strip_prefix_ci(target_part, "file:") {
            let (path, lines) = split_line_reference(rest.trim());
            if path.is_empty() {
                return None;
            }
            let (wiki, path) = split_wiki_prefix(path);
            return Some(WikiLink {
                target: LinkTarget::File { wiki, path: path.to_string(), lines },
                label,
                raw,
            });
        }

        // `symbol:` prefix (spec §29).
        if let Some(rest) = strip_prefix_ci(target_part, "symbol:") {
            let name = rest.trim();
            if name.is_empty() {
                return None;
            }
            return Some(WikiLink {
                target: LinkTarget::Symbol { name: name.to_string() },
                label,
                raw,
            });
        }

        // A page, possibly with `wiki::` and `#anchor`.
        let (wiki, rest) = split_wiki_prefix(target_part);
        let (name, anchor) = match rest.split_once('#') {
            Some((n, a)) => {
                let a = a.trim();
                (n.trim(), (!a.is_empty()).then(|| a.to_string()))
            }
            None => (rest.trim(), None),
        };

        if name.is_empty() && anchor.is_none() {
            return None;
        }

        Some(WikiLink {
            target: LinkTarget::Page { wiki, name: name.to_string(), anchor },
            label,
            raw,
        })
    }
}

/// Case-insensitively strips an **ASCII** prefix.
///
/// Comparison is done on bytes rather than by slicing the `str`, because
/// `&s[..prefix.len()]` panics when that byte index falls inside a multi-byte
/// character — which it does for any link whose name starts with non-ASCII
/// text, such as `[[Größe]]`. Once the ASCII bytes are known to match, the
/// split point is guaranteed to be a char boundary.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    debug_assert!(prefix.is_ascii(), "prefix must be ASCII for byte comparison");
    let head = s.as_bytes().get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix.as_bytes()) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Splits a leading `wiki::` prefix (spec §6).
fn split_wiki_prefix(s: &str) -> (Option<String>, &str) {
    match s.find("::") {
        // A leading `::` is not a wiki prefix, and neither is one that appears
        // after a path separator (`a/b::c` is a strange file name, not a wiki).
        Some(i) if i > 0 && !s[..i].contains('/') => {
            let wiki = s[..i].trim();
            let rest = s[i + 2..].trim_start();
            if wiki.is_empty() || rest.is_empty() {
                (None, s)
            } else {
                (Some(wiki.to_string()), rest)
            }
        }
        _ => (None, s),
    }
}

/// Splits a trailing `#L30` or `#L30-L55` line reference (spec §28).
fn split_line_reference(s: &str) -> (&str, Option<LineRange>) {
    let Some(hash) = s.rfind('#') else { return (s, None) };
    let (path, frag) = s.split_at(hash);
    let frag = &frag[1..];

    let Some(range) = parse_line_fragment(frag) else {
        // An unrecognised fragment stays part of the path rather than being
        // silently discarded — dropping it would resolve the wrong file.
        return (s, None);
    };
    (path, Some(range))
}

/// Parses `L30`, `L30-L55`, `30` or `30-55`.
fn parse_line_fragment(frag: &str) -> Option<LineRange> {
    let strip = |p: &str| -> Option<u32> {
        let p = p.trim();
        let digits = p.strip_prefix(['L', 'l']).unwrap_or(p);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok()
    };

    match frag.split_once('-') {
        Some((a, b)) => {
            let (start, end) = (strip(a)?, strip(b)?);
            // A reversed range is a typo, not a reason to fail: normalise it.
            Some(LineRange { start: start.min(end), end: start.max(end) })
        }
        None => strip(frag).map(LineRange::single),
    }
}

/// Finds every `[[...]]` link in a plain-text run.
///
/// This operates on a *single text run*, never on raw source: callers feed it
/// the text events a markdown parser has already produced, so `[[x]]` inside a
/// code span or fenced block is never treated as a link (spec §6, §30).
///
/// Returns each link together with its byte range within `text`.
pub fn find_links(text: &str) -> Vec<(std::ops::Range<usize>, WikiLink)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i + 1 < bytes.len() {
        if bytes[i] != b'[' || bytes[i + 1] != b'[' {
            i += 1;
            continue;
        }
        let content_start = i + 2;
        let Some(close) = find_closing(text, content_start) else {
            // Unclosed `[[` is ordinary text; stop looking for a pair here.
            i += 2;
            continue;
        };
        let inner = &text[content_start..close];
        if let Some(link) = WikiLink::parse_inner(inner) {
            out.push((i..close + 2, link));
        }
        i = close + 2;
    }
    out
}

/// Finds the `]]` that closes a link opened at `from`.
fn find_closing(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = from;
    while i + 1 < bytes.len() {
        if bytes[i] == b']' && bytes[i + 1] == b']' {
            return Some(i);
        }
        // A newline ends the link: wiki links do not span paragraphs.
        if bytes[i] == b'\n' {
            return None;
        }
        i += 1;
    }
    None
}

/// Normalises a heading into a GitHub-compatible anchor slug (spec §6).
pub fn heading_anchor(heading: &str) -> String {
    let mut out = String::with_capacity(heading.len());
    let mut last_dash = false;
    for c in heading.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            last_dash = false;
        } else if (c == '-' || c == '_' || c.is_whitespace()) && !out.is_empty() && !last_dash {
            out.push('-');
            last_dash = true;
        }
        // Everything else (punctuation, emoji) is dropped, as GitHub does.
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(inner: &str) -> WikiLink {
        WikiLink::parse_inner(inner).expect("should parse")
    }

    #[test]
    fn parses_a_simple_page_link() {
        let l = page("Heap Exploitation");
        assert_eq!(
            l.target,
            LinkTarget::Page { wiki: None, name: "Heap Exploitation".into(), anchor: None }
        );
        assert_eq!(l.display_text(), "Heap Exploitation");
    }

    #[test]
    fn parses_a_path_link() {
        let l = page("Security/Heap");
        assert_eq!(
            l.target,
            LinkTarget::Page { wiki: None, name: "Security/Heap".into(), anchor: None }
        );
    }

    #[test]
    fn parses_a_labelled_link() {
        let l = page("Security/Heap|Heap Exploitation");
        assert_eq!(l.label.as_deref(), Some("Heap Exploitation"));
        assert_eq!(l.display_text(), "Heap Exploitation");
        match l.target {
            LinkTarget::Page { name, .. } => assert_eq!(name, "Security/Heap"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_cross_wiki_links() {
        let l = page("rust::Ownership");
        assert_eq!(
            l.target,
            LinkTarget::Page { wiki: Some("rust".into()), name: "Ownership".into(), anchor: None }
        );
        assert_eq!(l.wiki(), Some("rust"));

        let l = page("crypto::AES");
        assert_eq!(l.wiki(), Some("crypto"));
        assert_eq!(l.display_text(), "AES");
    }

    #[test]
    fn parses_anchors() {
        let l = page("Heap#Tcache");
        assert_eq!(
            l.target,
            LinkTarget::Page {
                wiki: None,
                name: "Heap".into(),
                anchor: Some("Tcache".into())
            }
        );
        assert_eq!(l.display_text(), "Heap § Tcache");
    }

    #[test]
    fn parses_file_links_with_line_references() {
        let l = page("file:src/main.rs");
        assert_eq!(
            l.target,
            LinkTarget::File { wiki: None, path: "src/main.rs".into(), lines: None }
        );

        let l = page("file:src/main.rs#L30");
        assert_eq!(
            l.target,
            LinkTarget::File {
                wiki: None,
                path: "src/main.rs".into(),
                lines: Some(LineRange::single(30))
            }
        );
        assert_eq!(l.display_text(), "src/main.rs:30");

        let l = page("file:src/main.rs#L30-L55");
        assert_eq!(
            l.target,
            LinkTarget::File {
                wiki: None,
                path: "src/main.rs".into(),
                lines: Some(LineRange { start: 30, end: 55 })
            }
        );
        assert_eq!(l.display_text(), "src/main.rs:30-55");
    }

    #[test]
    fn accepts_line_references_without_the_l_prefix() {
        let l = page("file:a.rs#30-55");
        match l.target {
            LinkTarget::File { lines: Some(r), .. } => assert_eq!(r, LineRange { start: 30, end: 55 }),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_reversed_line_range_is_normalised() {
        let l = page("file:a.rs#L55-L30");
        match l.target {
            LinkTarget::File { lines: Some(r), .. } => assert_eq!(r, LineRange { start: 30, end: 55 }),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_unparseable_fragment_stays_part_of_the_path() {
        // Dropping it would silently open a different file.
        let l = page("file:weird#name.rs");
        match l.target {
            LinkTarget::File { path, lines, .. } => {
                assert_eq!(path, "weird#name.rs");
                assert!(lines.is_none());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_symbol_links() {
        let l = page("symbol:terminalwiki::index::Indexer");
        assert_eq!(
            l.target,
            LinkTarget::Symbol { name: "terminalwiki::index::Indexer".into() }
        );
    }

    #[test]
    fn cross_wiki_file_links_are_supported() {
        let l = page("file:rust::src/lib.rs");
        assert_eq!(
            l.target,
            LinkTarget::File { wiki: Some("rust".into()), path: "src/lib.rs".into(), lines: None }
        );
    }

    #[test]
    fn empty_links_are_not_links() {
        assert!(WikiLink::parse_inner("").is_none());
        assert!(WikiLink::parse_inner("   ").is_none());
        assert!(WikiLink::parse_inner("|label").is_none());
        assert!(WikiLink::parse_inner("file:").is_none());
        assert!(WikiLink::parse_inner("symbol:").is_none());
    }

    #[test]
    fn finds_links_within_a_text_run() {
        let text = "See [[Heap]] and [[Security/UAF|use after free]] for more.";
        let found = find_links(text);
        assert_eq!(found.len(), 2);
        assert_eq!(&text[found[0].0.clone()], "[[Heap]]");
        assert_eq!(found[1].1.display_text(), "use after free");
    }

    #[test]
    fn unclosed_links_are_plain_text() {
        assert!(find_links("an [[unclosed link").is_empty());
        assert!(find_links("[[").is_empty());
        assert!(find_links("]]").is_empty());
    }

    #[test]
    fn a_link_does_not_span_a_newline() {
        assert!(find_links("[[start\nend]]").is_empty());
    }

    #[test]
    fn adjacent_links_are_both_found() {
        let found = find_links("[[a]][[b]]");
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn unicode_link_names_have_correct_byte_ranges() {
        let text = "über [[Größe]] hier";
        let found = find_links(text);
        assert_eq!(found.len(), 1);
        assert_eq!(&text[found[0].0.clone()], "[[Größe]]");
    }

    #[test]
    fn heading_anchors_match_the_github_convention() {
        assert_eq!(heading_anchor("Memory allocator"), "memory-allocator");
        assert_eq!(heading_anchor("Tcache Poisoning!"), "tcache-poisoning");
        assert_eq!(heading_anchor("C++ and Rust"), "c-and-rust");
        assert_eq!(heading_anchor("  Spaced  Out  "), "spaced-out");
        assert_eq!(heading_anchor("Größe"), "größe");
        assert_eq!(heading_anchor("###"), "");
    }

    #[test]
    fn a_label_containing_syntax_characters_is_not_reinterpreted() {
        let l = page("Page|see file:x.rs#L1");
        assert_eq!(l.label.as_deref(), Some("see file:x.rs#L1"));
        match l.target {
            LinkTarget::Page { name, .. } => assert_eq!(name, "Page"),
            other => panic!("{other:?}"),
        }
    }
}
