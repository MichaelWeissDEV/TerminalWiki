//! Addressing and page resolution (spec §8, §10, §68, §73).
//!
//! # Startup cost
//!
//! Spec §68 requires `tw PAGE` to avoid a full wiki scan when the page can be
//! located directly. Resolution therefore runs in three escalating tiers, and
//! stops at the first that succeeds:
//!
//! 1. **Direct** — construct candidate paths from the name and `stat` them.
//!    This is O(1) syscalls and handles `tw heap`, `tw Security/Heap.md` and
//!    `tw src/main.rs`.
//! 2. **Sibling listing** — read only the *containing directory* and compare
//!    case-insensitively. One `readdir`, which handles `tw heap` finding
//!    `Heap.md`.
//! 3. **Deep scan** — walk the wiki, matching stems, titles and aliases. Only
//!    reached when the name is not a path-like reference at all, and it is the
//!    only tier that pays for the whole tree.

use std::path::{Path, PathBuf};

use crate::config::NamingPolicy;
use crate::error::{Error, Result};
use crate::filetype::{self, ContentType};
use crate::frontmatter::Frontmatter;
use crate::scan::{self, ScannedFile};
use crate::wiki::{Wiki, WikiSet};

/// Extensions tried when a name carries none, in priority order (spec §5, §6).
pub const RESOLVE_EXTENSIONS: &[&str] = &[
    "md", "markdown", "txt", "rst", "org", "tex", "rs", "py", "c", "h", "go", "sh",
];

/// What the user asked for on the command line (spec §8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// `tw` — the default wiki's home page.
    DefaultHome,
    /// `tw rust` where `rust` is a registered wiki — that wiki's home page.
    WikiHome { wiki: String },
    /// A page, optionally in a named wiki.
    Page { wiki: Option<String>, name: String },
}

/// How the address was written, so the CLI can explain ambiguity if needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressSource {
    /// Bare arguments, subject to the wiki-name-wins rule (spec §8).
    Positional,
    /// `@wiki page` — the wiki was named explicitly.
    Explicit,
    /// `tw page NAME` — forced to be a page (spec §8).
    ForcedPage,
}

impl Address {
    /// Parses CLI arguments into an address (spec §8).
    ///
    /// The disambiguation rule is fixed and deliberately free of heuristics:
    /// if the first argument is exactly a registered wiki name, it *is* the
    /// wiki. A page of the same name is reachable as `tw page NAME`.
    pub fn parse(args: &[String], wikis: &WikiSet, source: AddressSource) -> Result<Address> {
        // `@name` always denotes a wiki, wherever it appears first.
        if let Some(first) = args.first() {
            if let Some(name) = first.strip_prefix('@') {
                if name.is_empty() {
                    return Err(Error::invalid_arguments(
                        "`@` must be followed by a wiki name, for example `tw @rust ownership`",
                    ));
                }
                // Validate eagerly so `tw @nope page` reports the wiki, not the page.
                wikis.require(name)?;
                let rest = args[1..].join(" ");
                return Ok(if rest.trim().is_empty() {
                    Address::WikiHome {
                        wiki: name.to_string(),
                    }
                } else {
                    Address::Page {
                        wiki: Some(name.to_string()),
                        name: rest,
                    }
                });
            }
        }

        match args.len() {
            0 => Ok(Address::DefaultHome),
            _ => {
                let first = &args[0];
                let is_wiki = source == AddressSource::Positional && wikis.get(first).is_some();
                if is_wiki {
                    let rest = args[1..].join(" ");
                    return Ok(if rest.trim().is_empty() {
                        Address::WikiHome {
                            wiki: first.clone(),
                        }
                    } else {
                        Address::Page {
                            wiki: Some(first.clone()),
                            name: rest,
                        }
                    });
                }
                // Everything else is a page name in the default wiki. Joining
                // with spaces lets `tw heap exploitation` work unquoted.
                Ok(Address::Page {
                    wiki: None,
                    name: args.join(" "),
                })
            }
        }
    }
}

/// How a page was found, used for ranking and for explaining results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchKind {
    /// The name was an exact relative path.
    ExactPath,
    /// The name matched a file stem exactly.
    ExactStem,
    /// Matched ignoring case.
    CaseInsensitive,
    /// Matched after applying a naming policy (`Heap Exploitation` → `Heap-Exploitation`).
    Normalized,
    /// Matched an explicit frontmatter title.
    Title,
    /// Matched a frontmatter alias.
    Alias,
}

/// A resolved page.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub wiki: String,
    pub path: PathBuf,
    /// Path relative to the wiki root — the page's stable identity (spec §4).
    pub relative: PathBuf,
    pub kind: MatchKind,
}

/// Generates the direct candidate paths for a name, cheapest first.
///
/// This is pure string work; no filesystem access happens here, which keeps it
/// unit-testable and keeps tier 1 to a predictable handful of `stat` calls.
pub fn direct_candidates(name: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let trimmed = name.trim().trim_start_matches("./");
    if trimmed.is_empty() {
        return out;
    }

    let mut push = |p: PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };

    // A name that already carries a known extension is used verbatim.
    let has_known_ext = filetype::classify_by_extension(Path::new(trimmed)).is_some();
    if has_known_ext {
        push(PathBuf::from(trimmed));
    }

    // Name plus each candidate extension.
    for ext in RESOLVE_EXTENSIONS {
        push(PathBuf::from(format!("{trimmed}.{ext}")));
    }

    // Directory-style pages: `Security` → `Security/index.md`.
    for home in crate::wiki::HOME_CANDIDATES {
        push(PathBuf::from(trimmed).join(home));
    }

    // Naming-policy variants, so `tw "Heap Exploitation"` finds
    // `Heap-Exploitation.md` and `heap-exploitation.md` alike (spec §46).
    for policy in [
        NamingPolicy::KebabCase,
        NamingPolicy::SnakeCase,
        NamingPolicy::Slug,
    ] {
        let converted = apply_policy_preserving_dirs(policy, trimmed);
        if converted != trimmed {
            for ext in ["md", "markdown"] {
                push(PathBuf::from(format!("{converted}.{ext}")));
            }
        }
    }

    // Finally the bare name, for extensionless files such as `Makefile`.
    if !has_known_ext {
        push(PathBuf::from(trimmed));
    }
    out
}

/// Applies a naming policy to each path segment, leaving separators intact.
fn apply_policy_preserving_dirs(policy: NamingPolicy, name: &str) -> String {
    name.split('/')
        .map(|seg| policy.apply(seg))
        .collect::<Vec<_>>()
        .join("/")
}

/// Tier 1 + 2: resolve without walking the whole wiki (spec §68).
///
/// Returns `None` when only a deep scan could answer the question.
pub fn resolve_shallow(wiki: &Wiki, name: &str) -> Option<Resolution> {
    // Tier 1: direct stat of constructed candidates.
    for candidate in direct_candidates(name) {
        let Ok(full) = wiki.resolve_within(&candidate) else {
            continue;
        };
        if full.is_file() {
            let kind = if candidate.as_os_str() == name {
                MatchKind::ExactPath
            } else {
                MatchKind::ExactStem
            };
            return Some(Resolution {
                wiki: wiki.name.clone(),
                relative: wiki.relative(&full).to_path_buf(),
                path: full,
                kind,
            });
        }
    }

    // Tier 2: one directory listing, compared case-insensitively.
    let rel = Path::new(name.trim());
    let parent = rel.parent().unwrap_or(Path::new(""));
    let stem = rel.file_name()?.to_string_lossy().to_lowercase();
    let dir = wiki.resolve_within(parent).ok()?;
    let entries = std::fs::read_dir(&dir).ok()?;

    let mut best: Option<Resolution> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_lowercase();
        let file_stem = Path::new(&file_name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let matches = file_name == stem
            || file_stem == stem
            || file_stem == apply_policy_preserving_dirs(NamingPolicy::KebabCase, &stem)
            || file_stem == apply_policy_preserving_dirs(NamingPolicy::SnakeCase, &stem)
            || file_stem == apply_policy_preserving_dirs(NamingPolicy::Slug, &stem);

        if !matches {
            continue;
        }
        // Prefer markdown over other types when several files share a stem.
        let ct = filetype::classify_by_extension(&path).unwrap_or(ContentType::Text);
        let better = best.as_ref().is_none_or(|b| {
            let current = filetype::classify_by_extension(&b.path).unwrap_or(ContentType::Text);
            ct == ContentType::Markdown && current != ContentType::Markdown
        });
        if better {
            best = Some(Resolution {
                wiki: wiki.name.clone(),
                relative: wiki.relative(&path).to_path_buf(),
                path,
                kind: MatchKind::CaseInsensitive,
            });
        }
    }
    best
}

/// Tier 3: walk the wiki, matching stems, titles and aliases (spec §7, §13).
///
/// Every match is returned, ranked, so the caller can report an ambiguity
/// rather than silently picking one.
pub fn resolve_deep(
    wiki: &Wiki,
    name: &str,
    index_config: &crate::config::IndexConfig,
) -> Vec<Resolution> {
    let needle = name.trim().to_lowercase();
    let normalized: Vec<String> = [
        NamingPolicy::KebabCase,
        NamingPolicy::SnakeCase,
        NamingPolicy::Slug,
    ]
    .iter()
    .map(|p| p.apply(&needle).to_lowercase())
    .collect();

    let mut out = Vec::new();
    for file in scan::scan(wiki, index_config) {
        if let Some(kind) = match_file(wiki, &file, &needle, &normalized) {
            out.push(Resolution {
                wiki: wiki.name.clone(),
                relative: file.relative.clone(),
                path: file.path.clone(),
                kind,
            });
        }
    }
    // Best match kind first, then shortest path: a top-level page beats one
    // buried six directories deep.
    out.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| {
                a.relative
                    .components()
                    .count()
                    .cmp(&b.relative.components().count())
            })
            .then_with(|| a.relative.cmp(&b.relative))
    });
    out
}

fn match_file(
    _wiki: &Wiki,
    file: &ScannedFile,
    needle: &str,
    normalized: &[String],
) -> Option<MatchKind> {
    let stem = file.relative.file_stem()?.to_string_lossy().to_lowercase();
    let rel_str = file.relative.to_string_lossy().to_lowercase();
    let rel_no_ext = rel_str
        .rsplit_once('.')
        .map(|(a, _)| a.to_string())
        .unwrap_or(rel_str.clone());

    if rel_str == needle || rel_no_ext == needle {
        return Some(MatchKind::ExactPath);
    }
    if stem == needle {
        return Some(MatchKind::ExactStem);
    }
    if normalized.iter().any(|n| &stem == n || &rel_no_ext == n) {
        return Some(MatchKind::Normalized);
    }

    // Title and alias matching require reading the file's frontmatter, so it
    // is done last and only for text files of a sane size.
    if !file.should_index_content() || !file.content_type.is_page() {
        return None;
    }
    let text = std::fs::read_to_string(&file.path).ok()?;
    let fm = Frontmatter::parse(&text);
    if let Some(title) = &fm.title {
        if title.to_lowercase() == needle {
            return Some(MatchKind::Title);
        }
    }
    if fm.aliases.iter().any(|a| a.to_lowercase() == needle) {
        return Some(MatchKind::Alias);
    }
    // A first-level heading acts as an implicit title (spec §7).
    if fm.title.is_none() {
        if let Some(h1) = first_heading(&text[fm.body_offset.min(text.len())..]) {
            if h1.to_lowercase() == needle {
                return Some(MatchKind::Title);
            }
        }
    }
    None
}

/// Extracts the first ATX heading, used as an implicit page title (spec §7).
pub fn first_heading(text: &str) -> Option<String> {
    let mut in_fence = false;
    for line in text.lines().take(400) {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            let title = rest.trim().trim_end_matches('#').trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }
    None
}

/// The title to show for a page when no frontmatter title exists (spec §7).
///
/// Falls back to the file stem, so a page never displays as untitled.
pub fn default_title(relative: &Path, text: Option<&str>) -> String {
    if let Some(text) = text {
        let fm = Frontmatter::parse(text);
        if let Some(t) = fm.title {
            return t;
        }
        if let Some(h) = first_heading(&text[fm.body_offset.min(text.len())..]) {
            return h;
        }
    }
    relative
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| relative.to_string_lossy().to_string())
}

/// Resolves a page across a wiki and everything mounted into it (spec §8).
///
/// The host wiki is searched first at every tier, so a local page always wins
/// over a same-named page in a mounted subwiki.
pub fn resolve(
    wikis: &WikiSet,
    start: &str,
    name: &str,
    index_config: &crate::config::IndexConfig,
) -> Result<Resolution> {
    let order = wikis.search_order(start);
    if order.is_empty() {
        return Err(Error::WikiNotFound {
            name: start.to_string(),
            known: wikis.names(),
        });
    }

    // Tier 1+2 across every wiki before any deep scan: a cheap hit in a
    // mounted wiki still beats walking the host wiki (spec §68).
    for wiki in &order {
        if let Some(found) = resolve_shallow(wiki, name) {
            return Ok(found);
        }
    }

    for wiki in &order {
        let mut found = resolve_deep(wiki, name, index_config);
        if !found.is_empty() {
            return Ok(found.remove(0));
        }
    }

    Err(Error::PageNotFound {
        query: name.to_string(),
        wiki: Some(start.to_string()),
        suggestions: collect_suggestions(&order, name, index_config),
    })
}

/// Builds the "did you mean" list shown when resolution fails (spec §73).
fn collect_suggestions(
    order: &[&Wiki],
    name: &str,
    index_config: &crate::config::IndexConfig,
) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for wiki in order.iter().take(4) {
        for file in scan::scan(wiki, index_config) {
            if !file.content_type.is_page() {
                continue;
            }
            if let Some(stem) = file.relative.file_stem() {
                names.push(stem.to_string_lossy().to_string());
            }
        }
        // Enough candidates to make a good suggestion without scanning forever.
        if names.len() > 20_000 {
            break;
        }
    }
    let refs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    crate::fuzzy::suggestions(name, refs, 5)
        .into_iter()
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, IndexConfig, WikiEntry};
    use std::fs;

    fn tmpdir(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "tw-resolve-{}-{}-{:?}",
            tag,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn wiki_at(base: &Path) -> Wiki {
        Wiki::open(&WikiEntry {
            name: "main".into(),
            path: base.to_path_buf(),
            mounts: vec![],
        })
        .unwrap()
    }

    fn set_with(entries: &[(&str, &Path, Vec<&str>)], default: &str) -> WikiSet {
        let mut cfg = Config::default();
        for (name, path, mounts) in entries {
            cfg.wikis.push(WikiEntry {
                name: name.to_string(),
                path: path.to_path_buf(),
                mounts: mounts.iter().map(|s| s.to_string()).collect(),
            });
        }
        cfg.default_wiki = Some(default.to_string());
        let (set, errs) = WikiSet::open(&cfg);
        assert!(errs.is_empty(), "{errs:?}");
        set
    }

    // --- address parsing (spec §8) -----------------------------------------

    #[test]
    fn bare_tw_addresses_the_default_home() {
        let base = tmpdir("addr-home");
        let set = set_with(&[("main", &base, vec![])], "main");
        assert_eq!(
            Address::parse(&[], &set, AddressSource::Positional).unwrap(),
            Address::DefaultHome
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_wiki_name_wins_over_a_page_of_the_same_name() {
        let base = tmpdir("addr-wiki");
        let rust = base.join("rust");
        fs::create_dir_all(&rust).unwrap();
        // A page literally named `rust` also exists in main.
        fs::write(base.join("rust.md"), "# The page").unwrap();
        let set = set_with(&[("main", &base, vec![]), ("rust", &rust, vec![])], "main");

        let a = Address::parse(&["rust".into()], &set, AddressSource::Positional).unwrap();
        assert_eq!(
            a,
            Address::WikiHome {
                wiki: "rust".into()
            }
        );

        // `tw page rust` is the documented escape hatch.
        let a = Address::parse(&["rust".into()], &set, AddressSource::ForcedPage).unwrap();
        assert_eq!(
            a,
            Address::Page {
                wiki: None,
                name: "rust".into()
            }
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn wiki_plus_page_addresses_a_page_inside_that_wiki() {
        let base = tmpdir("addr-wp");
        let rust = base.join("rust");
        fs::create_dir_all(&rust).unwrap();
        let set = set_with(&[("main", &base, vec![]), ("rust", &rust, vec![])], "main");
        let a = Address::parse(
            &["rust".into(), "ownership".into()],
            &set,
            AddressSource::Positional,
        )
        .unwrap();
        assert_eq!(
            a,
            Address::Page {
                wiki: Some("rust".into()),
                name: "ownership".into()
            }
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn at_syntax_addresses_a_wiki_explicitly() {
        let base = tmpdir("addr-at");
        let rust = base.join("rust");
        fs::create_dir_all(&rust).unwrap();
        let set = set_with(&[("main", &base, vec![]), ("rust", &rust, vec![])], "main");

        let a = Address::parse(
            &["@rust".into(), "ownership".into()],
            &set,
            AddressSource::Positional,
        )
        .unwrap();
        assert_eq!(
            a,
            Address::Page {
                wiki: Some("rust".into()),
                name: "ownership".into()
            }
        );

        let a = Address::parse(&["@rust".into()], &set, AddressSource::Positional).unwrap();
        assert_eq!(
            a,
            Address::WikiHome {
                wiki: "rust".into()
            }
        );
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn at_syntax_reports_an_unknown_wiki_rather_than_a_missing_page() {
        let base = tmpdir("addr-atbad");
        let set = set_with(&[("main", &base, vec![])], "main");
        let err = Address::parse(
            &["@nope".into(), "x".into()],
            &set,
            AddressSource::Positional,
        )
        .unwrap_err();
        assert_eq!(err.exit_code(), crate::ExitCode::WikiNotFound);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn multiword_page_names_need_no_quoting() {
        let base = tmpdir("addr-multi");
        let set = set_with(&[("main", &base, vec![])], "main");
        let a = Address::parse(
            &["heap".into(), "exploitation".into()],
            &set,
            AddressSource::Positional,
        )
        .unwrap();
        assert_eq!(
            a,
            Address::Page {
                wiki: None,
                name: "heap exploitation".into()
            }
        );
        fs::remove_dir_all(&base).ok();
    }

    // --- candidate generation ----------------------------------------------

    #[test]
    fn direct_candidates_cover_the_documented_shapes() {
        let c = direct_candidates("heap");
        assert!(c.contains(&PathBuf::from("heap.md")));
        assert!(c.contains(&PathBuf::from("heap/index.md")));
        assert!(c.contains(&PathBuf::from("heap/README.md")));

        let c = direct_candidates("Heap Exploitation");
        assert!(c.contains(&PathBuf::from("Heap-Exploitation.md")));
        assert!(c.contains(&PathBuf::from("Heap_Exploitation.md")));
        assert!(c.contains(&PathBuf::from("heap-exploitation.md")));

        // A path with a known extension is tried verbatim and first.
        let c = direct_candidates("src/main.rs");
        assert_eq!(c[0], PathBuf::from("src/main.rs"));
    }

    #[test]
    fn candidates_preserve_directory_separators_under_naming_policies() {
        let c = direct_candidates("Security/Heap Exploitation");
        assert!(c.contains(&PathBuf::from("Security/Heap-Exploitation.md")));
    }

    // --- shallow resolution (spec §68) --------------------------------------

    #[test]
    fn shallow_resolution_finds_an_exact_file() {
        let base = tmpdir("shallow-exact");
        fs::write(base.join("heap.md"), "# Heap").unwrap();
        let w = wiki_at(&base);
        let r = resolve_shallow(&w, "heap").unwrap();
        assert!(r.path.ends_with("heap.md"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn shallow_resolution_is_case_insensitive() {
        let base = tmpdir("shallow-case");
        fs::write(base.join("Heap.md"), "# Heap").unwrap();
        let w = wiki_at(&base);
        let r = resolve_shallow(&w, "heap").expect("case-insensitive match");
        assert!(r.path.ends_with("Heap.md"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn shallow_resolution_handles_nested_paths() {
        let base = tmpdir("shallow-nested");
        fs::create_dir_all(base.join("Security")).unwrap();
        fs::write(base.join("Security/Heap.md"), "# Heap").unwrap();
        let w = wiki_at(&base);
        assert!(resolve_shallow(&w, "Security/Heap").is_some());
        assert!(resolve_shallow(&w, "Security/Heap.md").is_some());
        assert!(resolve_shallow(&w, "security/heap").is_some());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn shallow_resolution_finds_directory_index_pages() {
        let base = tmpdir("shallow-dir");
        fs::create_dir_all(base.join("Security")).unwrap();
        fs::write(base.join("Security/index.md"), "# Security").unwrap();
        let w = wiki_at(&base);
        let r = resolve_shallow(&w, "Security").unwrap();
        assert!(r.path.ends_with("index.md"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn shallow_resolution_finds_naming_policy_variants() {
        let base = tmpdir("shallow-policy");
        fs::write(base.join("Heap-Exploitation.md"), "# X").unwrap();
        let w = wiki_at(&base);
        assert!(resolve_shallow(&w, "Heap Exploitation").is_some());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn markdown_wins_when_several_files_share_a_stem() {
        let base = tmpdir("shallow-prefer");
        fs::write(base.join("heap.txt"), "text").unwrap();
        fs::write(base.join("heap.md"), "# md").unwrap();
        let w = wiki_at(&base);
        let r = resolve_shallow(&w, "heap").unwrap();
        assert!(r.path.ends_with("heap.md"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn code_files_resolve_as_first_class_pages() {
        let base = tmpdir("shallow-code");
        fs::create_dir_all(base.join("src")).unwrap();
        fs::write(base.join("src/main.rs"), "fn main() {}").unwrap();
        let w = wiki_at(&base);
        let r = resolve_shallow(&w, "src/main.rs").unwrap();
        assert!(r.path.ends_with("main.rs"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn resolution_cannot_escape_the_wiki_root() {
        let base = tmpdir("shallow-escape");
        let outside = base.join("outside");
        let wiki = base.join("wiki");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(&wiki).unwrap();
        fs::write(outside.join("secret.md"), "secret").unwrap();
        let w = Wiki::open(&WikiEntry {
            name: "m".into(),
            path: wiki,
            mounts: vec![],
        })
        .unwrap();
        assert!(resolve_shallow(&w, "../outside/secret").is_none());
        assert!(resolve_shallow(&w, "../outside/secret.md").is_none());
        fs::remove_dir_all(&base).ok();
    }

    // --- deep resolution ----------------------------------------------------

    #[test]
    fn deep_resolution_matches_a_frontmatter_title() {
        let base = tmpdir("deep-title");
        fs::create_dir_all(base.join("s")).unwrap();
        fs::write(
            base.join("s/h.md"),
            "---\ntitle: Heap Exploitation\n---\n\nbody\n",
        )
        .unwrap();
        let w = wiki_at(&base);
        let found = resolve_deep(&w, "Heap Exploitation", &IndexConfig::default());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, MatchKind::Title);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn deep_resolution_matches_an_alias() {
        let base = tmpdir("deep-alias");
        fs::write(
            base.join("h.md"),
            "---\ntitle: Heap Exploitation\naliases:\n  - malloc exploitation\n---\nbody\n",
        )
        .unwrap();
        let w = wiki_at(&base);
        let found = resolve_deep(&w, "malloc exploitation", &IndexConfig::default());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, MatchKind::Alias);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn deep_resolution_matches_a_first_heading_as_implicit_title() {
        let base = tmpdir("deep-h1");
        fs::write(base.join("random-name.md"), "# Memory Allocators\n\nbody\n").unwrap();
        let w = wiki_at(&base);
        let found = resolve_deep(&w, "Memory Allocators", &IndexConfig::default());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, MatchKind::Title);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_heading_inside_a_code_fence_is_not_a_title() {
        let text = "```\n# Not a title\n```\n\n# Real Title\n";
        assert_eq!(first_heading(text), Some("Real Title".to_string()));
    }

    #[test]
    fn shallower_pages_outrank_deeply_nested_ones() {
        let base = tmpdir("deep-rank");
        fs::create_dir_all(base.join("a/b/c")).unwrap();
        fs::write(base.join("a/b/c/thing.md"), "x").unwrap();
        fs::write(base.join("thing.md"), "x").unwrap();
        let w = wiki_at(&base);
        let found = resolve_deep(&w, "thing", &IndexConfig::default());
        assert_eq!(found[0].relative, PathBuf::from("thing.md"));
        fs::remove_dir_all(&base).ok();
    }

    // --- across mounts (spec §8) -------------------------------------------

    #[test]
    fn a_local_page_wins_over_a_mounted_one() {
        let base = tmpdir("mount-shadow");
        let main = base.join("main");
        let rust = base.join("rust");
        fs::create_dir_all(&main).unwrap();
        fs::create_dir_all(&rust).unwrap();
        fs::write(main.join("ownership.md"), "# Local").unwrap();
        fs::write(rust.join("ownership.md"), "# Mounted").unwrap();

        let set = set_with(
            &[("main", &main, vec!["rust"]), ("rust", &rust, vec![])],
            "main",
        );
        let r = resolve(&set, "main", "ownership", &IndexConfig::default()).unwrap();
        assert_eq!(r.wiki, "main");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_page_is_found_in_a_mounted_subwiki() {
        let base = tmpdir("mount-find");
        let main = base.join("main");
        let rust = base.join("rust");
        fs::create_dir_all(&main).unwrap();
        fs::create_dir_all(&rust).unwrap();
        fs::write(rust.join("ownership.md"), "# Ownership").unwrap();

        let set = set_with(
            &[("main", &main, vec!["rust"]), ("rust", &rust, vec![])],
            "main",
        );
        let r = resolve(&set, "main", "ownership", &IndexConfig::default()).unwrap();
        assert_eq!(r.wiki, "rust");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_missing_page_produces_suggestions() {
        let base = tmpdir("suggest");
        fs::write(base.join("Heap.md"), "# Heap").unwrap();
        fs::write(base.join("Heap-Exploitation.md"), "# Heap Exploitation").unwrap();
        fs::write(base.join("Compiler.md"), "# Compiler").unwrap();
        let set = set_with(&[("main", &base, vec![])], "main");

        let err = resolve(&set, "main", "heep", &IndexConfig::default()).unwrap_err();
        match err {
            Error::PageNotFound { suggestions, .. } => {
                assert!(suggestions.iter().any(|s| s == "Heap"), "{suggestions:?}");
            }
            other => panic!("wrong error: {other:?}"),
        }
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn default_title_falls_back_through_the_documented_chain() {
        let p = Path::new("security/Heap.md");
        assert_eq!(
            default_title(p, Some("---\ntitle: From Frontmatter\n---\n# H1")),
            "From Frontmatter"
        );
        assert_eq!(default_title(p, Some("# From Heading\n")), "From Heading");
        assert_eq!(default_title(p, Some("no heading here")), "Heap");
        assert_eq!(default_title(p, None), "Heap");
    }
}
