//! Wikis, logical subwiki mounts, and path containment (spec §8, §42).
//!
//! A wiki *is* a directory (spec §4). Mounting one wiki into another is a
//! purely logical operation: no file is copied, moved or rewritten, and the
//! mounted wiki remains an independent, self-sufficient directory.
//!
//! Path containment is enforced here rather than at each call site, because
//! every asset reference, wiki link and CLI argument eventually becomes a path
//! and any one of them could otherwise reach `../../../../etc/shadow`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use crate::config::{Config, WikiEntry, WikiLocalConfig};
use crate::error::{Error, Result};

/// Candidate home-page names, in the order the spec requires (spec §10).
pub const HOME_CANDIDATES: &[&str] = &["index.md", "README.md", "Home.md", "home.md"];

/// A wiki that has been opened: its registry entry plus its local config.
#[derive(Debug, Clone)]
pub struct Wiki {
    pub name: String,
    pub root: PathBuf,
    /// Names of wikis logically mounted into this one (spec §8).
    pub mounts: Vec<String>,
    /// Contents of `.terminalwiki.toml`, if present (spec §82).
    pub local: WikiLocalConfig,
}

impl Wiki {
    /// Opens a wiki from its registry entry, reading its local configuration.
    ///
    /// The root must exist and be a directory; anything else is a
    /// configuration error the user needs to see rather than a silent empty
    /// wiki.
    pub fn open(entry: &WikiEntry) -> Result<Wiki> {
        let root = &entry.path;
        let meta = std::fs::metadata(root).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Config {
                    message: format!("wiki `{}` points at a missing path", entry.name),
                    path: Some(root.clone()),
                }
            } else {
                Error::io(root, e)
            }
        })?;
        if !meta.is_dir() {
            return Err(Error::Config {
                message: format!("wiki `{}` must point at a directory", entry.name),
                path: Some(root.clone()),
            });
        }
        // Canonicalise once, so every later containment check compares against
        // a path that has no symlinks or `..` left in it.
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.clone());
        let local = WikiLocalConfig::load(&root)?;
        Ok(Wiki { name: entry.name.clone(), root, mounts: entry.mounts.clone(), local })
    }

    /// The wiki's display title (spec §82), defaulting to its name.
    pub fn title(&self) -> &str {
        self.local.title.as_deref().unwrap_or(&self.name)
    }

    /// Locates the wiki's home page (spec §10).
    ///
    /// The standard names are tried first, then any `home` from
    /// `.terminalwiki.toml`. Returns `None` when the wiki has no home page, in
    /// which case the caller shows a generated overview rather than failing.
    pub fn home_page(&self) -> Option<PathBuf> {
        if let Some(home) = &self.local.home {
            if let Ok(p) = self.resolve_within(Path::new(home)) {
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        for name in HOME_CANDIDATES {
            let candidate = self.root.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    /// Joins a wiki-relative path, refusing anything that escapes the root.
    ///
    /// This is the single choke point for turning untrusted relative paths —
    /// from markdown links, asset references or the command line — into real
    /// paths (spec §42).
    pub fn resolve_within(&self, rel: &Path) -> Result<PathBuf> {
        resolve_within(&self.root, rel)
    }

    /// Returns `path` expressed relative to the wiki root, for display.
    pub fn relative<'a>(&self, path: &'a Path) -> &'a Path {
        path.strip_prefix(&self.root).unwrap_or(path)
    }

    /// True if `path` lies inside this wiki.
    pub fn contains(&self, path: &Path) -> bool {
        path.starts_with(&self.root)
    }
}

/// Joins `rel` onto `root`, guaranteeing the result stays inside `root`.
///
/// Two independent checks are applied, because either alone is insufficient:
///
/// 1. **Lexical.** `rel` must be relative and must never rise above its own
///    starting point. This rejects `../../etc/passwd` before touching the
///    filesystem, which matters because the target may not exist yet.
/// 2. **Canonical.** If the joined path exists, it is canonicalised and
///    checked against the canonical root. This is what catches a *symlink*
///    inside the wiki pointing outside it — a case the lexical check cannot
///    see (spec §42).
pub fn resolve_within(root: &Path, rel: &Path) -> Result<PathBuf> {
    if rel.is_absolute() {
        return Err(Error::PathRefused {
            path: rel.to_path_buf(),
            reason: "absolute paths are not allowed inside a wiki".into(),
        });
    }

    // Lexical normalisation without touching the filesystem.
    let mut depth = 0isize;
    let mut normalized = PathBuf::new();
    for component in rel.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return Err(Error::PathRefused {
                        path: rel.to_path_buf(),
                        reason: "path escapes the wiki root".into(),
                    });
                }
                normalized.pop();
            }
            Component::Normal(part) => {
                depth += 1;
                normalized.push(part);
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(Error::PathRefused {
                    path: rel.to_path_buf(),
                    reason: "absolute paths are not allowed inside a wiki".into(),
                });
            }
        }
    }

    let joined = root.join(&normalized);

    // Canonical check: only meaningful once the path exists.
    if joined.exists() {
        let canonical = std::fs::canonicalize(&joined).map_err(|e| Error::io(&joined, e))?;
        let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        if !canonical.starts_with(&canonical_root) {
            return Err(Error::PathRefused {
                path: rel.to_path_buf(),
                reason: "path leaves the wiki root through a symbolic link".into(),
            });
        }
        return Ok(canonical);
    }

    Ok(joined)
}

/// Canonicalises a path that may not exist yet.
///
/// Falls back to canonicalising the nearest existing ancestor and re-appending
/// the remainder, so a path to a file that has not been created still compares
/// correctly against a canonical wiki root.
pub fn canonicalize_lenient(path: &Path) -> PathBuf {
    if let Ok(c) = std::fs::canonicalize(path) {
        return c;
    }
    let mut suffix = Vec::new();
    let mut cursor = path;
    while let Some(parent) = cursor.parent() {
        if let Some(name) = cursor.file_name() {
            suffix.push(name.to_os_string());
        }
        if let Ok(c) = std::fs::canonicalize(parent) {
            let mut out = c;
            for part in suffix.iter().rev() {
                out.push(part);
            }
            return out;
        }
        cursor = parent;
    }
    path.to_path_buf()
}

/// Every configured wiki, plus the mount graph between them (spec §8).
#[derive(Debug, Clone)]
pub struct WikiSet {
    wikis: BTreeMap<String, Wiki>,
    default_name: Option<String>,
}

impl WikiSet {
    /// Opens every wiki named in the configuration.
    ///
    /// A wiki whose directory is missing is reported but does not prevent the
    /// others from working: one broken entry must not make the tool unusable.
    pub fn open(config: &Config) -> (WikiSet, Vec<Error>) {
        let mut wikis = BTreeMap::new();
        let mut errors = Vec::new();
        for entry in &config.wikis {
            match Wiki::open(entry) {
                Ok(w) => {
                    wikis.insert(w.name.clone(), w);
                }
                Err(e) => errors.push(e),
            }
        }
        let default_name = config
            .default_wiki
            .clone()
            .filter(|n| wikis.contains_key(n))
            .or_else(|| config.wikis.first().map(|w| w.name.clone()).filter(|n| wikis.contains_key(n)));
        (WikiSet { wikis, default_name }, errors)
    }

    pub fn is_empty(&self) -> bool {
        self.wikis.is_empty()
    }

    pub fn len(&self) -> usize {
        self.wikis.len()
    }

    pub fn get(&self, name: &str) -> Option<&Wiki> {
        self.wikis.get(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.wikis.keys().cloned().collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Wiki> {
        self.wikis.values()
    }

    pub fn default_wiki(&self) -> Option<&Wiki> {
        self.default_name.as_ref().and_then(|n| self.wikis.get(n))
    }

    /// Looks up a wiki by name, producing a helpful error when absent.
    pub fn require(&self, name: &str) -> Result<&Wiki> {
        self.wikis.get(name).ok_or_else(|| Error::WikiNotFound {
            name: name.to_string(),
            known: self.names(),
        })
    }

    /// The wiki to use when the user named none.
    pub fn require_default(&self) -> Result<&Wiki> {
        self.default_wiki().ok_or(Error::NoWikiConfigured)
    }

    /// The search order for a wiki and everything mounted into it (spec §8).
    ///
    /// The host wiki always comes first, so a locally defined page wins over a
    /// same-named page in a mounted subwiki. Mount cycles are tolerated: each
    /// wiki appears exactly once.
    pub fn search_order(&self, start: &str) -> Vec<&Wiki> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut order = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(start.to_string());
        while let Some(name) = queue.pop_front() {
            let Some(wiki) = self.wikis.get(&name) else { continue };
            if !seen.insert(&wiki.name) {
                continue;
            }
            order.push(wiki);
            for m in &wiki.mounts {
                queue.push_back(m.clone());
            }
        }
        order
    }

    /// Names of mounts that are configured but not registered (spec §71).
    pub fn missing_mounts(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for wiki in self.wikis.values() {
            for m in &wiki.mounts {
                if !self.wikis.contains_key(m) {
                    out.push((wiki.name.clone(), m.clone()));
                }
            }
        }
        out
    }

    /// Finds which wiki a path belongs to, preferring the most specific root.
    ///
    /// Wiki roots are stored canonicalised, so the incoming path is
    /// canonicalised too before comparing. Without this, a perfectly valid
    /// path would fail to match on any system where the wiki lives under a
    /// symlinked directory — `/tmp` on macOS being the everyday example.
    pub fn wiki_for_path(&self, path: &Path) -> Option<&Wiki> {
        let canonical = canonicalize_lenient(path);
        self.wikis
            .values()
            .filter(|w| canonical.starts_with(&w.root))
            .max_by_key(|w| w.root.as_os_str().len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Creates a throwaway directory tree for a test.
    fn tmpdir(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "tw-wiki-test-{}-{}-{:?}",
            tag,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn traversal_is_refused_lexically() {
        let root = Path::new("/wiki");
        for evil in ["../etc/passwd", "../../etc/shadow", "a/../../b", "./../x"] {
            let err = resolve_within(root, Path::new(evil)).unwrap_err();
            assert!(
                matches!(err, Error::PathRefused { .. }),
                "{evil} was not refused: {err:?}"
            );
        }
    }

    #[test]
    fn absolute_paths_are_refused() {
        let err = resolve_within(Path::new("/wiki"), Path::new("/etc/shadow")).unwrap_err();
        assert!(matches!(err, Error::PathRefused { .. }));
    }

    #[test]
    fn interior_parent_segments_that_stay_inside_are_allowed() {
        // `a/../b` never leaves the root, so it is legitimate.
        let got = resolve_within(Path::new("/wiki"), Path::new("a/../b")).unwrap();
        assert_eq!(got, PathBuf::from("/wiki/b"));
        let got = resolve_within(Path::new("/wiki"), Path::new("./x/./y")).unwrap();
        assert_eq!(got, PathBuf::from("/wiki/x/y"));
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_pointing_outside_the_wiki_is_refused() {
        let base = tmpdir("symlink");
        let wiki = base.join("wiki");
        let outside = base.join("outside");
        fs::create_dir_all(&wiki).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.md"), "top secret").unwrap();

        // A link inside the wiki that escapes it. The lexical check cannot see
        // this; only canonicalisation catches it.
        std::os::unix::fs::symlink(&outside, wiki.join("escape")).unwrap();

        let err = resolve_within(&wiki, Path::new("escape/secret.md")).unwrap_err();
        assert!(matches!(err, Error::PathRefused { .. }), "symlink escape allowed: {err:?}");

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_staying_inside_the_wiki_is_allowed() {
        let base = tmpdir("symlink-ok");
        let wiki = base.join("wiki");
        fs::create_dir_all(wiki.join("real")).unwrap();
        fs::write(wiki.join("real/page.md"), "# Page").unwrap();
        std::os::unix::fs::symlink(wiki.join("real"), wiki.join("link")).unwrap();

        let got = resolve_within(&wiki, Path::new("link/page.md")).unwrap();
        assert!(got.ends_with("real/page.md"));

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn nonexistent_paths_still_resolve_for_creation() {
        // `tw new` needs a path for a file that does not exist yet.
        let got = resolve_within(Path::new("/wiki"), Path::new("new/page.md")).unwrap();
        assert_eq!(got, PathBuf::from("/wiki/new/page.md"));
    }

    #[test]
    fn home_page_follows_the_documented_priority() {
        let base = tmpdir("home");
        // README before Home, index before everything.
        fs::write(base.join("Home.md"), "home").unwrap();
        let entry = WikiEntry { name: "t".into(), path: base.clone(), mounts: vec![] };
        let w = Wiki::open(&entry).unwrap();
        assert!(w.home_page().unwrap().ends_with("Home.md"));

        fs::write(base.join("README.md"), "readme").unwrap();
        let w = Wiki::open(&entry).unwrap();
        assert!(w.home_page().unwrap().ends_with("README.md"));

        fs::write(base.join("index.md"), "index").unwrap();
        let w = Wiki::open(&entry).unwrap();
        assert!(w.home_page().unwrap().ends_with("index.md"));

        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_wiki_without_a_home_page_is_not_an_error() {
        let base = tmpdir("nohome");
        let entry = WikiEntry { name: "t".into(), path: base.clone(), mounts: vec![] };
        let w = Wiki::open(&entry).unwrap();
        assert!(w.home_page().is_none());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn configured_home_is_used_when_no_standard_name_exists() {
        let base = tmpdir("confhome");
        fs::write(base.join("Start.md"), "start").unwrap();
        fs::write(base.join(".terminalwiki.toml"), "home = \"Start.md\"\n").unwrap();
        let entry = WikiEntry { name: "t".into(), path: base.clone(), mounts: vec![] };
        let w = Wiki::open(&entry).unwrap();
        assert!(w.home_page().unwrap().ends_with("Start.md"));
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn a_configured_home_cannot_escape_the_wiki() {
        let base = tmpdir("evilhome");
        fs::write(base.join(".terminalwiki.toml"), "home = \"../../../etc/passwd\"\n").unwrap();
        let entry = WikiEntry { name: "t".into(), path: base.clone(), mounts: vec![] };
        let w = Wiki::open(&entry).unwrap();
        assert!(w.home_page().is_none(), "escaping home path was accepted");
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn missing_wiki_directory_is_a_configuration_error() {
        let entry = WikiEntry {
            name: "gone".into(),
            path: PathBuf::from("/nonexistent/tw/path/xyz"),
            mounts: vec![],
        };
        let err = Wiki::open(&entry).unwrap_err();
        assert_eq!(err.exit_code(), crate::ExitCode::ConfigError);
    }

    // --- mount graph -------------------------------------------------------

    fn set_with(entries: Vec<(&str, Vec<&str>)>) -> (WikiSet, PathBuf) {
        let base = tmpdir("mounts");
        let mut cfg = Config::default();
        for (name, mounts) in entries {
            let path = base.join(name);
            fs::create_dir_all(&path).unwrap();
            cfg.wikis.push(WikiEntry {
                name: name.to_string(),
                path,
                mounts: mounts.into_iter().map(String::from).collect(),
            });
        }
        cfg.default_wiki = Some("main".into());
        let (set, errs) = WikiSet::open(&cfg);
        assert!(errs.is_empty(), "{errs:?}");
        (set, base)
    }

    #[test]
    fn search_order_puts_the_host_wiki_first() {
        let (set, base) = set_with(vec![
            ("main", vec!["rust", "security"]),
            ("rust", vec![]),
            ("security", vec![]),
        ]);
        let order: Vec<&str> = set.search_order("main").iter().map(|w| w.name.as_str()).collect();
        assert_eq!(order, vec!["main", "rust", "security"]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn mount_cycles_terminate() {
        // A mounts B, B mounts A. Resolution must not loop forever.
        let (set, base) = set_with(vec![("main", vec!["rust"]), ("rust", vec!["main"])]);
        let order: Vec<&str> = set.search_order("main").iter().map(|w| w.name.as_str()).collect();
        assert_eq!(order, vec!["main", "rust"]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn self_mount_terminates() {
        let (set, base) = set_with(vec![("main", vec!["main"])]);
        assert_eq!(set.search_order("main").len(), 1);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn missing_mounts_are_reported_for_lint() {
        let (set, base) = set_with(vec![("main", vec!["ghost"])]);
        assert_eq!(set.missing_mounts(), vec![("main".to_string(), "ghost".to_string())]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn mounting_never_copies_files() {
        // The mounted wiki's files stay exactly where they were.
        let (set, base) = set_with(vec![("main", vec!["rust"]), ("rust", vec![])]);
        let rust = set.get("rust").unwrap();
        fs::write(rust.root.join("ownership.md"), "# Ownership").unwrap();
        let main = set.get("main").unwrap();
        assert!(!main.root.join("ownership.md").exists(), "mount copied a file");
        assert!(rust.root.join("ownership.md").exists());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn wiki_for_path_prefers_the_most_specific_root() {
        let base = tmpdir("nested");
        let outer = base.join("outer");
        let inner = outer.join("inner");
        fs::create_dir_all(&inner).unwrap();
        let mut cfg = Config::default();
        cfg.wikis.push(WikiEntry { name: "outer".into(), path: outer.clone(), mounts: vec![] });
        cfg.wikis.push(WikiEntry { name: "inner".into(), path: inner.clone(), mounts: vec![] });
        let (set, _) = WikiSet::open(&cfg);
        let found = set.wiki_for_path(&inner.join("page.md")).unwrap();
        assert_eq!(found.name, "inner");
        fs::remove_dir_all(&base).ok();
    }
}
