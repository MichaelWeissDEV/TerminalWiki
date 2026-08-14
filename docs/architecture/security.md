# Security Architecture

## 1. Wiki Root Confinement & Path Traversal Prevention

Every path resolved from CLI arguments, markdown links (`[text](url)`), or wiki links (`[[...]]`) is validated against path traversal:
- Canonical base directory comparisons prevent `../../` escapes.
- Symlinks pointing outside the configured wiki root are rejected.

## 2. Terminal Escape Sanitization

Untrusted markdown text, titles, frontmatter, filenames, search results, and backlink snippets are passed through the sanitizer before output:
- Strips harmful ANSI/OSC escape sequences (e.g. OSC 52 clipboard hijacking, OSC 8 hyperlinks, DCS, CSI).
- Null bytes and bidirectional override characters are neutralized.

## 3. External Editor Trust Boundary

When launching `$EDITOR` (or `$VISUAL`/`TW_EDITOR`):
- Terminal raw mode and alternate screen are cleanly dropped via RAII guards.
- Subprocess arguments are tokenized without shell injection.
- Terminal state is restored on editor exit.
