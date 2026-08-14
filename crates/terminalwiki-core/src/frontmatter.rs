//! Optional YAML frontmatter (spec §7).
//!
//! Frontmatter is *optional* by design: a page without it must work exactly as
//! well as one with it, and TerminalWiki never writes metadata back into a
//! file (spec §7 forbids `last_opened`-style churn that would pollute git
//! diffs).
//!
//! # Why a hand-written parser
//!
//! The frontmatter surface the spec actually uses is `title`, `aliases` and
//! `tags` — strings and lists of strings. `serde_yaml` is archived upstream,
//! and pulling a full YAML engine in for three keys contradicts the dependency
//! discipline of spec §112 while adding startup cost (spec §68). This parser
//! covers the documented subset, reports precise diagnostics for `tw lint`
//! (spec §71), and refuses to silently accept things it does not understand.

use std::collections::BTreeMap;

/// A parsed frontmatter value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Scalar(String),
    List(Vec<String>),
}

impl Value {
    pub fn as_scalar(&self) -> Option<&str> {
        match self {
            Value::Scalar(s) => Some(s),
            Value::List(_) => None,
        }
    }

    /// Reads a value as a list, treating a lone scalar as a one-element list.
    ///
    /// `tags: security` and `tags: [security]` mean the same thing to a human,
    /// so they mean the same thing here.
    pub fn as_list(&self) -> Vec<String> {
        match self {
            Value::Scalar(s) if s.is_empty() => Vec::new(),
            Value::Scalar(s) => vec![s.clone()],
            Value::List(v) => v.clone(),
        }
    }
}

/// A non-fatal frontmatter problem, surfaced by `tw lint` (spec §71).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// 1-based line number within the file.
    pub line: usize,
    pub message: String,
}

/// Parsed frontmatter plus everything needed to locate the body.
#[derive(Debug, Clone, Default)]
pub struct Frontmatter {
    /// Explicit title, if given.
    pub title: Option<String>,
    /// Alternative names this page can be resolved by (spec §13).
    pub aliases: Vec<String>,
    /// Tags (spec §7, §14).
    pub tags: Vec<String>,
    /// Every other key, preserved but not interpreted.
    pub extra: BTreeMap<String, Value>,
    /// Byte offset of the document body within the original text.
    pub body_offset: usize,
    /// Number of lines the frontmatter block occupies, for line mapping.
    pub body_line_offset: usize,
    /// Whether a frontmatter block was present at all.
    pub present: bool,
    /// Problems found while parsing. Never fatal.
    pub diagnostics: Vec<Diagnostic>,
}

impl Frontmatter {
    /// Parses leading frontmatter, returning defaults when none is present.
    ///
    /// Only a `---` fence on the very first line opens a block, matching the
    /// convention every markdown tool uses. A missing closing fence is
    /// reported and the whole file is treated as body, never swallowed.
    pub fn parse(text: &str) -> Frontmatter {
        let mut fm = Frontmatter::default();

        let rest = match strip_opening_fence(text) {
            Some(r) => r,
            None => return fm,
        };
        let fence_len = text.len() - rest.len();

        let Some((block, body_start_in_rest, block_lines)) = find_closing_fence(rest) else {
            fm.diagnostics.push(Diagnostic {
                line: 1,
                message: "frontmatter block is never closed by `---`".into(),
            });
            return fm;
        };

        fm.present = true;
        fm.body_offset = fence_len + body_start_in_rest;
        // +1 for the opening fence line, +1 for the closing fence line.
        fm.body_line_offset = block_lines + 2;
        parse_block(block, &mut fm);
        fm
    }

    /// True when the block carried no usable information.
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.aliases.is_empty() && self.tags.is_empty()
    }
}

/// Consumes a leading `---` line, tolerating a UTF-8 BOM and CRLF.
fn strip_opening_fence(text: &str) -> Option<&str> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let rest = text.strip_prefix("---")?;
    // The fence line must contain nothing else.
    let rest = rest.strip_prefix('\r').unwrap_or(rest);
    rest.strip_prefix('\n')
}

/// Finds the closing `---` (or `...`) fence.
///
/// Returns the block contents, the byte offset of the body within `rest`, and
/// the number of lines in the block.
fn find_closing_fence(rest: &str) -> Option<(&str, usize, usize)> {
    let mut offset = 0usize;
    for (lines, line) in rest.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" || trimmed == "..." {
            return Some((&rest[..offset], offset + line.len(), lines));
        }
        offset += line.len();
    }
    None
}

fn parse_block(block: &str, fm: &mut Frontmatter) {
    let mut pending_list_key: Option<String> = None;
    let mut pending_list: Vec<String> = Vec::new();
    // Line numbers are offset by 1 because line 1 is the opening fence.
    let mut lineno = 1usize;

    let mut entries: Vec<(String, Value, usize)> = Vec::new();

    for raw in block.lines() {
        lineno += 1;
        let line = raw.trim_end();
        let trimmed = line.trim_start();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // A list item belonging to the previous `key:` line.
        if let Some(item) = trimmed.strip_prefix("- ").or_else(|| {
            // A bare `-` yields an empty item, which we report rather than keep.
            (trimmed == "-").then_some("")
        }) {
            if pending_list_key.is_some() {
                let value = unquote(item.trim());
                if value.is_empty() {
                    fm.diagnostics.push(Diagnostic {
                        line: lineno,
                        message: "empty list item".into(),
                    });
                } else {
                    pending_list.push(value);
                }
            } else {
                fm.diagnostics.push(Diagnostic {
                    line: lineno,
                    message: "list item without a preceding key".into(),
                });
            }
            continue;
        }

        // Any other indented line is nesting we do not model.
        let indented = line.starts_with(' ') || line.starts_with('\t');

        let Some(colon) = find_key_separator(trimmed) else {
            fm.diagnostics.push(Diagnostic {
                line: lineno,
                message: format!("expected `key: value`, found `{}`", truncate(trimmed, 40)),
            });
            continue;
        };

        if indented {
            fm.diagnostics.push(Diagnostic {
                line: lineno,
                message: "nested mappings are not supported in frontmatter".into(),
            });
            continue;
        }

        // A new key closes any list that was being collected.
        if let Some(key) = pending_list_key.take() {
            entries.push((key, Value::List(std::mem::take(&mut pending_list)), lineno));
        }

        let key = trimmed[..colon].trim().to_string();
        let value = trimmed[colon + 1..].trim();

        if key.is_empty() {
            fm.diagnostics.push(Diagnostic {
                line: lineno,
                message: "empty key".into(),
            });
            continue;
        }

        if value.is_empty() {
            // Either an upcoming block list, or an explicitly empty value.
            pending_list_key = Some(key);
            continue;
        }

        if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            let items: Vec<String> = inner
                .split(',')
                .map(|s| unquote(s.trim()))
                .filter(|s| !s.is_empty())
                .collect();
            entries.push((key, Value::List(items), lineno));
        } else {
            entries.push((key, Value::Scalar(unquote(value)), lineno));
        }
    }

    if let Some(key) = pending_list_key.take() {
        entries.push((key, Value::List(std::mem::take(&mut pending_list)), lineno));
    }

    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for (key, value, line) in entries {
        let lower = key.to_ascii_lowercase();
        if let Some(prev) = seen.insert(lower.clone(), line) {
            fm.diagnostics.push(Diagnostic {
                line,
                message: format!("duplicate key `{key}` (first seen on line {prev})"),
            });
        }
        match lower.as_str() {
            "title" => match value.as_scalar() {
                Some(s) => fm.title = Some(s.to_string()),
                None => fm.diagnostics.push(Diagnostic {
                    line,
                    message: "`title` must be a single value, not a list".into(),
                }),
            },
            "alias" | "aliases" => fm.aliases.extend(value.as_list()),
            "tag" | "tags" => fm.tags.extend(value.as_list()),
            _ => {
                fm.extra.insert(key, value);
            }
        }
    }

    // Tags are normalised for matching but kept human-readable for display.
    fm.tags = dedupe(
        fm.tags
            .drain(..)
            .map(|t| t.trim_start_matches('#').trim().to_string()),
    );
    fm.aliases = dedupe(fm.aliases.drain(..).map(|a| a.trim().to_string()));
}

/// Finds the `:` that separates a key from its value.
///
/// Skips colons inside quotes so `title: "a: b"` parses correctly.
fn find_key_separator(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut quote: Option<u8> = None;
    for (i, &b) in bytes.iter().enumerate() {
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b':' => return Some(i),
            None => {}
        }
    }
    None
}

fn dedupe(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        if item.is_empty() {
            continue;
        }
        if !out.iter().any(|e| e.eq_ignore_ascii_case(&item)) {
            out.push(item);
        }
    }
    out
}

/// Removes matching surrounding quotes and trailing comments.
fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 {
        let b = s.as_bytes();
        if (b[0] == b'"' && b[b.len() - 1] == b'"') || (b[0] == b'\'' && b[b.len() - 1] == b'\'') {
            return s[1..s.len() - 1].to_string();
        }
    }
    // An unquoted value may carry a trailing comment.
    match s.find(" #") {
        Some(i) => s[..i].trim().to_string(),
        None => s.to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_spec_example() {
        let src = "---\ntitle: Heap Exploitation\naliases:\n  - Heap\n  - malloc exploitation\ntags:\n  - security\n  - exploitation\n  - linux\n---\n\n# Body\n";
        let fm = Frontmatter::parse(src);
        assert!(fm.present);
        assert_eq!(fm.title.as_deref(), Some("Heap Exploitation"));
        assert_eq!(fm.aliases, vec!["Heap", "malloc exploitation"]);
        assert_eq!(fm.tags, vec!["security", "exploitation", "linux"]);
        assert!(fm.diagnostics.is_empty(), "{:?}", fm.diagnostics);
        assert_eq!(&src[fm.body_offset..], "\n# Body\n");
    }

    #[test]
    fn a_page_without_frontmatter_is_fully_valid() {
        let src = "# Just a heading\n\nText.\n";
        let fm = Frontmatter::parse(src);
        assert!(!fm.present);
        assert!(fm.is_empty());
        assert!(fm.diagnostics.is_empty());
        assert_eq!(fm.body_offset, 0);
        assert_eq!(&src[fm.body_offset..], src);
    }

    #[test]
    fn inline_lists_are_supported() {
        let fm = Frontmatter::parse("---\ntags: [security, linux]\n---\nbody");
        assert_eq!(fm.tags, vec!["security", "linux"]);
    }

    #[test]
    fn a_scalar_tag_is_treated_as_a_single_tag() {
        let fm = Frontmatter::parse("---\ntags: security\n---\nbody");
        assert_eq!(fm.tags, vec!["security"]);
    }

    #[test]
    fn quoted_values_may_contain_colons() {
        let fm = Frontmatter::parse("---\ntitle: \"memory: management\"\n---\nx");
        assert_eq!(fm.title.as_deref(), Some("memory: management"));
    }

    #[test]
    fn unclosed_frontmatter_is_reported_and_not_swallowed() {
        let src = "---\ntitle: Broken\n\n# Heading\n";
        let fm = Frontmatter::parse(src);
        assert!(!fm.present);
        assert_eq!(fm.body_offset, 0, "body must not be lost");
        assert_eq!(fm.diagnostics.len(), 1);
        assert!(fm.diagnostics[0].message.contains("never closed"));
    }

    #[test]
    fn duplicate_keys_are_reported() {
        let fm = Frontmatter::parse("---\ntitle: A\ntitle: B\n---\nx");
        assert!(fm
            .diagnostics
            .iter()
            .any(|d| d.message.contains("duplicate key")));
    }

    #[test]
    fn garbage_lines_are_reported_but_do_not_abort_parsing() {
        let fm = Frontmatter::parse("---\nthis is not yaml\ntags: [a]\n---\nx");
        assert_eq!(fm.tags, vec!["a"]);
        assert_eq!(fm.diagnostics.len(), 1);
        assert_eq!(fm.diagnostics[0].line, 2);
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let fm = Frontmatter::parse("---\n# a comment\n\ntitle: X\n---\nbody");
        assert_eq!(fm.title.as_deref(), Some("X"));
        assert!(fm.diagnostics.is_empty());
    }

    #[test]
    fn tags_are_deduplicated_case_insensitively() {
        let fm = Frontmatter::parse("---\ntags: [Security, security, SECURITY]\n---\nx");
        assert_eq!(fm.tags, vec!["Security"]);
    }

    #[test]
    fn leading_hash_on_tags_is_stripped() {
        let fm = Frontmatter::parse("---\ntags: [\"#security\"]\n---\nx");
        assert_eq!(fm.tags, vec!["security"]);
    }

    #[test]
    fn crlf_frontmatter_parses() {
        let fm = Frontmatter::parse("---\r\ntitle: X\r\n---\r\nbody");
        assert!(fm.present);
        assert_eq!(fm.title.as_deref(), Some("X"));
    }

    #[test]
    fn a_horizontal_rule_in_the_body_is_not_frontmatter() {
        let src = "# Title\n\n---\n\nmore\n";
        let fm = Frontmatter::parse(src);
        assert!(!fm.present);
    }

    #[test]
    fn unknown_keys_are_preserved_not_dropped() {
        let fm = Frontmatter::parse("---\nauthor: Ada\n---\nx");
        assert_eq!(
            fm.extra.get("author").and_then(|v| v.as_scalar()),
            Some("Ada")
        );
    }

    #[test]
    fn body_line_offset_maps_body_lines_back_to_the_file() {
        let src = "---\ntitle: X\n---\nline one\n";
        let fm = Frontmatter::parse(src);
        // Opening fence, `title`, closing fence => body starts on file line 4.
        assert_eq!(fm.body_line_offset, 3);
    }
}
