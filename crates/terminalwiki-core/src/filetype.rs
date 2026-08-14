//! Content classification (spec §5).
//!
//! The extension table below is a *fast path*, not a gate. Any file whose
//! contents sniff as UTF-8 text is viewable, whether or not its extension is
//! known — the spec explicitly forbids requiring a small fixed extension list.
//! Conversely, no file that sniffs as binary is ever written to the terminal
//! as text, regardless of its extension.

use std::path::Path;

/// The broad content class used for display decisions and for `type:` queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    /// Markdown, the primary wiki format.
    Markdown,
    /// Plain prose / lightweight markup that is not markdown.
    Text,
    /// Source code, a first-class document (spec §27).
    Code,
    /// LaTeX or BibTeX source.
    Latex,
    /// A raster or vector image.
    Image,
    /// Not text; only metadata is ever shown (spec §5).
    Binary,
}

impl ContentType {
    pub fn as_str(self) -> &'static str {
        match self {
            ContentType::Markdown => "markdown",
            ContentType::Text => "text",
            ContentType::Code => "code",
            ContentType::Latex => "latex",
            ContentType::Image => "image",
            ContentType::Binary => "binary",
        }
    }

    /// Parses the value used by `--type` and by `type:` in queries.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "markdown" | "md" => Some(ContentType::Markdown),
            "text" | "txt" => Some(ContentType::Text),
            "code" => Some(ContentType::Code),
            "latex" | "tex" => Some(ContentType::Latex),
            "image" | "img" => Some(ContentType::Image),
            "binary" | "bin" => Some(ContentType::Binary),
            _ => None,
        }
    }

    /// Whether this class participates in the wiki's page namespace.
    pub fn is_page(self) -> bool {
        matches!(self, ContentType::Markdown | ContentType::Text)
    }

    /// Whether full text is worth indexing for this class.
    pub fn is_indexable_text(self) -> bool {
        !matches!(self, ContentType::Binary | ContentType::Image)
    }
}

/// Markdown extensions (spec §6).
pub const MARKDOWN_EXTENSIONS: &[&str] = &["md", "markdown", "mdown", "mkd"];

/// Plain-text / lightweight-markup extensions (spec §5).
pub const TEXT_EXTENSIONS: &[&str] = &["txt", "rst", "org", "adoc", "asciidoc", "text"];

/// Image extensions (spec §5).
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "svg", "bmp", "avif"];

/// LaTeX-family extensions (spec §5, §32).
pub const LATEX_EXTENSIONS: &[&str] = &["tex", "bib", "sty", "cls"];

/// Extension to highlighting language name (spec §5, §27).
///
/// Used to label the viewer and to pick a syntax definition later. Unknown
/// extensions fall back to plain text and never produce an error (spec §30).
pub const CODE_LANGUAGES: &[(&str, &str)] = &[
    ("rs", "Rust"),
    ("c", "C"),
    ("h", "C"),
    ("cpp", "C++"),
    ("cc", "C++"),
    ("cxx", "C++"),
    ("hpp", "C++"),
    ("hh", "C++"),
    ("py", "Python"),
    ("go", "Go"),
    ("java", "Java"),
    ("kt", "Kotlin"),
    ("kts", "Kotlin"),
    ("js", "JavaScript"),
    ("jsx", "JavaScript"),
    ("mjs", "JavaScript"),
    ("cjs", "JavaScript"),
    ("ts", "TypeScript"),
    ("tsx", "TypeScript"),
    ("lua", "Lua"),
    ("sh", "Shell"),
    ("bash", "Shell"),
    ("zsh", "Shell"),
    ("fish", "Fish"),
    ("asm", "Assembly"),
    ("s", "Assembly"),
    ("S", "Assembly"),
    ("sql", "SQL"),
    ("html", "HTML"),
    ("htm", "HTML"),
    ("css", "CSS"),
    ("scss", "SCSS"),
    ("json", "JSON"),
    ("yaml", "YAML"),
    ("yml", "YAML"),
    ("toml", "TOML"),
    ("xml", "XML"),
    ("rb", "Ruby"),
    ("php", "PHP"),
    ("pl", "Perl"),
    ("swift", "Swift"),
    ("scala", "Scala"),
    ("hs", "Haskell"),
    ("ml", "OCaml"),
    ("ex", "Elixir"),
    ("exs", "Elixir"),
    ("erl", "Erlang"),
    ("zig", "Zig"),
    ("nim", "Nim"),
    ("dart", "Dart"),
    ("vim", "VimL"),
    ("nix", "Nix"),
    ("make", "Makefile"),
    ("mk", "Makefile"),
    ("cmake", "CMake"),
    ("dockerfile", "Dockerfile"),
    ("proto", "Protobuf"),
    ("diff", "Diff"),
    ("patch", "Diff"),
];

/// File names without a useful extension that are still known code.
const CODE_FILENAMES: &[(&str, &str)] = &[
    ("Makefile", "Makefile"),
    ("makefile", "Makefile"),
    ("GNUmakefile", "Makefile"),
    ("Dockerfile", "Dockerfile"),
    ("CMakeLists.txt", "CMake"),
    ("Justfile", "Just"),
    ("justfile", "Just"),
];

/// Returns the lowercase extension of `path`, if any.
pub fn extension(path: &Path) -> Option<String> {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase())
}

/// The highlighting language name for a path, if it is recognised as code.
pub fn language_for(path: &Path) -> Option<&'static str> {
    if let Some(name) = path.file_name().map(|n| n.to_string_lossy()) {
        if let Some((_, lang)) = CODE_FILENAMES.iter().find(|(f, _)| *f == name) {
            return Some(lang);
        }
    }
    let ext = extension(path)?;
    CODE_LANGUAGES.iter().find(|(e, _)| *e == ext).map(|(_, lang)| *lang)
}

/// The highlighting language name for a fenced-code-block info string (spec §30).
pub fn language_for_tag(tag: &str) -> Option<&'static str> {
    // Fences carry attributes in several dialects: `rust,no_run`, `rust ignore`,
    // `c{.numberLines}`. Only the leading language token is significant.
    let tag = tag
        .split([',', ' ', '\t', '{', ';'])
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if tag.is_empty() {
        return None;
    }
    if let Some((_, lang)) = CODE_LANGUAGES.iter().find(|(e, _)| *e == tag) {
        return Some(lang);
    }
    // Common aliases used in fences that are not file extensions.
    let alias = match tag.as_str() {
        "rust" => "Rust",
        "python" | "python3" => "Python",
        "javascript" | "node" => "JavaScript",
        "typescript" => "TypeScript",
        "shell" | "console" | "terminal" => "Shell",
        "c++" => "C++",
        "csharp" | "cs" => "C#",
        "golang" => "Go",
        "yaml" => "YAML",
        "markdown" => "Markdown",
        _ => return None,
    };
    Some(alias)
}

/// Classifies by extension alone, without touching the file.
///
/// Returns `None` when the extension carries no information, in which case the
/// caller should sniff the contents.
pub fn classify_by_extension(path: &Path) -> Option<ContentType> {
    if language_for(path).is_some() {
        return Some(ContentType::Code);
    }
    let ext = extension(path)?;
    if MARKDOWN_EXTENSIONS.contains(&ext.as_str()) {
        return Some(ContentType::Markdown);
    }
    if TEXT_EXTENSIONS.contains(&ext.as_str()) {
        return Some(ContentType::Text);
    }
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        return Some(ContentType::Image);
    }
    if LATEX_EXTENSIONS.contains(&ext.as_str()) {
        return Some(ContentType::Latex);
    }
    None
}

/// How many leading bytes are inspected when sniffing for binary content.
pub const SNIFF_LEN: usize = 8192;

/// Returns true when `bytes` should be treated as binary (spec §5).
///
/// The rule is deliberately simple and conservative:
///
/// * A NUL byte in the sniffed prefix means binary. No text format the wiki
///   cares about contains one, and NUL is the single most reliable signal.
/// * Otherwise, if the prefix does not decode as UTF-8 — allowing for a
///   multi-byte character truncated by the sniff window — it is binary.
///
/// This means a UTF-8 file with unusual content is still readable, while a
/// `.md` file that is secretly a JPEG is not dumped to the terminal.
pub fn looks_binary(bytes: &[u8]) -> bool {
    let prefix = &bytes[..bytes.len().min(SNIFF_LEN)];
    if prefix.contains(&0) {
        return true;
    }
    match std::str::from_utf8(prefix) {
        Ok(_) => false,
        Err(e) => {
            // A character cut in half by the sniff window is not an error, but
            // only if we actually truncated the file.
            let truncated = bytes.len() > SNIFF_LEN;
            match e.error_len() {
                // Unexpected end of input: fine if we truncated.
                None => !truncated,
                // A genuinely invalid sequence.
                Some(_) => true,
            }
        }
    }
}

/// Full classification, sniffing contents when the extension is uninformative.
pub fn classify(path: &Path, bytes: &[u8]) -> ContentType {
    match classify_by_extension(path) {
        // Images are identified by extension; their bytes are binary by nature.
        Some(ContentType::Image) => ContentType::Image,
        Some(known) => {
            // An extension claims text, but the bytes disagree. Trust the bytes.
            if looks_binary(bytes) {
                ContentType::Binary
            } else {
                known
            }
        }
        None => {
            if looks_binary(bytes) {
                ContentType::Binary
            } else {
                // Unknown extension, valid UTF-8: viewable as text (spec §5).
                ContentType::Text
            }
        }
    }
}

/// Classifies a file on disk by inspecting its extension and sniffing leading bytes.
pub fn classify_file(path: &Path) -> std::io::Result<ContentType> {
    use std::io::Read;
    if let Some(known) = classify_by_extension(path) {
        if known == ContentType::Image {
            return Ok(ContentType::Image);
        }
    }
    let mut file = std::fs::File::open(path)?;
    let mut buf = [0u8; SNIFF_LEN];
    let n = file.read(&mut buf)?;
    Ok(classify(path, &buf[..n]))
}

/// Formats a byte count the way the spec's binary-file block shows it (spec §5).
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Parses a human-written size such as `16 MiB` from configuration (spec §54).
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let num: f64 = num.trim().parse().ok()?;
    if num < 0.0 {
        return None;
    }
    let mult: u64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024 * 1024,
        "g" | "gb" | "gib" => 1024 * 1024 * 1024,
        _ => return None,
    };
    Some((num * mult as f64) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn markdown_and_code_are_classified_by_extension() {
        assert_eq!(classify_by_extension(&PathBuf::from("a.md")), Some(ContentType::Markdown));
        assert_eq!(classify_by_extension(&PathBuf::from("a.rs")), Some(ContentType::Code));
        assert_eq!(classify_by_extension(&PathBuf::from("a.png")), Some(ContentType::Image));
        assert_eq!(classify_by_extension(&PathBuf::from("a.tex")), Some(ContentType::Latex));
        assert_eq!(classify_by_extension(&PathBuf::from("a.txt")), Some(ContentType::Text));
        assert_eq!(classify_by_extension(&PathBuf::from("a.unknownext")), None);
    }

    #[test]
    fn makefiles_are_code_without_an_extension() {
        assert_eq!(language_for(&PathBuf::from("/x/Makefile")), Some("Makefile"));
        assert_eq!(language_for(&PathBuf::from("/x/Dockerfile")), Some("Dockerfile"));
    }

    #[test]
    fn unknown_extension_with_utf8_content_is_viewable_text() {
        // The spec forbids requiring a fixed extension list.
        let t = classify(&PathBuf::from("notes.weird"), "hello wörld".as_bytes());
        assert_eq!(t, ContentType::Text);
    }

    #[test]
    fn nul_bytes_mean_binary_even_with_a_text_extension() {
        let t = classify(&PathBuf::from("fake.md"), b"text\x00\x01\x02more");
        assert_eq!(t, ContentType::Binary);
    }

    #[test]
    fn invalid_utf8_is_binary() {
        assert!(looks_binary(&[0xFF, 0xFE, 0xFD, 0xFC]));
        assert!(!looks_binary("plain ascii".as_bytes()));
        assert!(!looks_binary("ünïcödé ✓ 漢字".as_bytes()));
    }

    #[test]
    fn a_multibyte_char_cut_by_the_sniff_window_is_not_binary() {
        // Build content longer than SNIFF_LEN whose boundary splits a char.
        let mut data = vec![b'a'; SNIFF_LEN - 1];
        data.extend_from_slice("漢".as_bytes()); // 3 bytes, split at the window
        data.extend(std::iter::repeat_n(b'b', 100));
        assert!(!looks_binary(&data), "truncated multi-byte char misread as binary");
    }

    #[test]
    fn empty_files_are_not_binary() {
        assert!(!looks_binary(b""));
    }

    #[test]
    fn fence_tags_resolve_including_aliases() {
        assert_eq!(language_for_tag("rust"), Some("Rust"));
        assert_eq!(language_for_tag("rs"), Some("Rust"));
        assert_eq!(language_for_tag("c++"), Some("C++"));
        assert_eq!(language_for_tag("rust,ignore"), Some("Rust"));
        // Unknown language must not error, just yield no highlighting.
        assert_eq!(language_for_tag("brainfuck-x"), None);
        assert_eq!(language_for_tag(""), None);
    }

    #[test]
    fn human_size_matches_the_spec_example() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(15_523_020), "14.8 MiB");
    }

    #[test]
    fn size_parsing_accepts_configuration_spellings() {
        assert_eq!(parse_size("16 MiB"), Some(16 * 1024 * 1024));
        assert_eq!(parse_size("16MiB"), Some(16 * 1024 * 1024));
        assert_eq!(parse_size("1024"), Some(1024));
        assert_eq!(parse_size("2 GiB"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_size("nonsense"), None);
        assert_eq!(parse_size("-5 MiB"), None);
    }
}
