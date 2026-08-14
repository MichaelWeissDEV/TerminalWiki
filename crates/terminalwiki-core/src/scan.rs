//! Walking a wiki directory (spec §53, §54).
//!
//! Scanning honours `.gitignore` and the TerminalWiki-specific `.twignore`,
//! plus any globs from configuration. Files above `max_file_size` are still
//! *listed* — the user must be able to see that a 2 GiB log exists — but are
//! flagged so nothing tries to read them into memory (spec §54).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ignore::WalkBuilder;

use crate::config::IndexConfig;
use crate::filetype::{self, ContentType};
use crate::wiki::Wiki;

/// The TerminalWiki-specific ignore file (spec §53).
pub const IGNORE_FILE: &str = ".twignore";

/// Directories that are never part of a knowledge base.
const ALWAYS_IGNORED_DIRS: &[&str] = &[".git", ".hg", ".svn", ".jj"];

/// One file discovered by a scan.
#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// Absolute path on disk.
    pub path: PathBuf,
    /// Path relative to the wiki root; the page's stable identity (spec §4).
    pub relative: PathBuf,
    pub size: u64,
    /// Modification time in whole seconds since the Unix epoch.
    pub mtime: u64,
    /// Classification based on extension. Contents are not read during a scan.
    pub content_type: ContentType,
    /// True when the file exceeds `max_file_size` (spec §54).
    pub too_large: bool,
}

impl ScannedFile {
    /// Whether this file's text should be read and indexed.
    pub fn should_index_content(&self) -> bool {
        !self.too_large && self.content_type.is_indexable_text()
    }
}

/// Returns modification time in seconds, or 0 when unavailable.
pub fn mtime_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Formats a Unix timestamp the way the spec's binary-file block shows it.
///
/// Uses UTC and the civil-from-days algorithm rather than pulling in a date
/// library for one format string (spec §112).
pub fn format_timestamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

/// Walks a wiki, yielding every file that survives the ignore rules.
///
/// The walk is single-threaded and streaming: callers that want parallelism
/// collect first and then use rayon, which keeps ordering deterministic for
/// tests (spec §3 — normal threads, no async runtime).
pub fn scan(wiki: &Wiki, config: &IndexConfig) -> Vec<ScannedFile> {
    let mut builder = WalkBuilder::new(&wiki.root);
    builder
        .hidden(!config.hidden)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        // A knowledge base is frequently *not* a git repository, and spec §53
        // asks for `.gitignore` rules to be honoured regardless. Without this,
        // `ignore` would silently skip them outside a repo.
        .require_git(false)
        .parents(false)
        .follow_links(false);
    builder.add_custom_ignore_filename(IGNORE_FILE);

    // Globs from global config and from `.terminalwiki.toml` (spec §82).
    let mut overrides = ignore::overrides::OverrideBuilder::new(&wiki.root);
    let mut have_overrides = false;
    for glob in config.ignore.iter().chain(wiki.local.ignore.iter()) {
        // An override glob is an *allow* rule; `!` makes it an ignore rule.
        if overrides.add(&format!("!{glob}")).is_ok() {
            have_overrides = true;
        }
    }
    if have_overrides {
        if let Ok(o) = overrides.build() {
            builder.overrides(o);
        }
    }

    let mut out = Vec::new();
    for entry in builder.build() {
        let Ok(entry) = entry else { continue };
        let path = entry.path();

        if entry.file_type().is_some_and(|t| t.is_dir()) {
            continue;
        }
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            // Skip symlinks, sockets and devices rather than following them.
            continue;
        }
        if path.components().any(|c| {
            matches!(c, std::path::Component::Normal(n)
                if ALWAYS_IGNORED_DIRS.iter().any(|d| n == std::ffi::OsStr::new(d)))
        }) {
            continue;
        }
        let Some(name) = path.file_name() else { continue };
        if name == crate::config::WIKI_CONFIG_FILE || name == IGNORE_FILE {
            continue;
        }

        let Ok(meta) = entry.metadata() else { continue };
        let Ok(relative) = path.strip_prefix(&wiki.root) else { continue };

        let content_type = filetype::classify_by_extension(path).unwrap_or(ContentType::Text);
        if content_type == ContentType::Code && !config.code {
            continue;
        }

        let size = meta.len();
        out.push(ScannedFile {
            path: path.to_path_buf(),
            relative: relative.to_path_buf(),
            size,
            mtime: mtime_secs(&meta),
            content_type,
            too_large: size > config.max_file_size,
        });
    }
    out.sort_by(|a, b| a.relative.cmp(&b.relative));
    out
}

/// Reads a file, refusing to load more than `limit` bytes (spec §54).
///
/// Returns the bytes and whether the file was truncated.
pub fn read_limited(path: &Path, limit: u64) -> std::io::Result<(Vec<u8>, bool)> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    if size <= limit {
        let mut buf = Vec::with_capacity(size as usize);
        let mut file = file;
        file.read_to_end(&mut buf)?;
        return Ok((buf, false));
    }
    let mut buf = vec![0u8; limit as usize];
    let mut handle = file.take(limit);
    let read = handle.read(&mut buf)?;
    buf.truncate(read);
    Ok((buf, true))
}

/// Wall-clock helper used by the index to record when it last ran.
pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WikiEntry;
    use std::fs;

    fn tmpdir(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "tw-scan-{}-{}-{:?}",
            tag,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        base
    }

    fn open(base: &Path) -> Wiki {
        Wiki::open(&WikiEntry { name: "t".into(), path: base.to_path_buf(), mounts: vec![] })
            .unwrap()
    }

    fn names(files: &[ScannedFile]) -> Vec<String> {
        files.iter().map(|f| f.relative.to_string_lossy().replace('\\', "/")).collect()
    }

    #[test]
    fn scans_nested_markdown_and_code() {
        let base = tmpdir("basic");
        fs::create_dir_all(base.join("Security")).unwrap();
        fs::write(base.join("index.md"), "# Index").unwrap();
        fs::write(base.join("Security/Heap.md"), "# Heap").unwrap();
        fs::write(base.join("allocator.rs"), "fn main() {}").unwrap();

        let files = scan(&open(&base), &IndexConfig::default());
        assert_eq!(names(&files), vec!["Security/Heap.md", "allocator.rs", "index.md"]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn gitignore_is_respected() {
        let base = tmpdir("gitignore");
        fs::write(base.join(".gitignore"), "build/\n*.o\n").unwrap();
        fs::create_dir_all(base.join("build")).unwrap();
        fs::write(base.join("build/out.md"), "x").unwrap();
        fs::write(base.join("obj.o"), "x").unwrap();
        fs::write(base.join("keep.md"), "x").unwrap();

        let files = scan(&open(&base), &IndexConfig::default());
        assert_eq!(names(&files), vec!["keep.md"]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn twignore_is_respected() {
        let base = tmpdir("twignore");
        fs::write(base.join(IGNORE_FILE), "captures/\n").unwrap();
        fs::create_dir_all(base.join("captures")).unwrap();
        fs::write(base.join("captures/a.md"), "x").unwrap();
        fs::write(base.join("b.md"), "x").unwrap();

        let files = scan(&open(&base), &IndexConfig::default());
        assert_eq!(names(&files), vec!["b.md"]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn wiki_local_ignore_globs_are_applied() {
        let base = tmpdir("localignore");
        fs::write(base.join(".terminalwiki.toml"), "ignore = [\"drafts/\"]\n").unwrap();
        fs::create_dir_all(base.join("drafts")).unwrap();
        fs::write(base.join("drafts/wip.md"), "x").unwrap();
        fs::write(base.join("final.md"), "x").unwrap();

        let files = scan(&open(&base), &IndexConfig::default());
        assert_eq!(names(&files), vec!["final.md"]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn the_git_directory_is_never_scanned() {
        let base = tmpdir("git");
        fs::create_dir_all(base.join(".git")).unwrap();
        fs::write(base.join(".git/COMMIT_EDITMSG"), "x").unwrap();
        fs::write(base.join("a.md"), "x").unwrap();

        let cfg = IndexConfig { hidden: true, ..Default::default() };
        let files = scan(&open(&base), &cfg);
        assert_eq!(names(&files), vec!["a.md"]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn config_files_are_not_pages() {
        let base = tmpdir("cfgfiles");
        fs::write(base.join(".terminalwiki.toml"), "title = \"x\"\n").unwrap();
        fs::write(base.join(IGNORE_FILE), "\n").unwrap();
        fs::write(base.join("a.md"), "x").unwrap();
        let cfg = IndexConfig { hidden: true, ..Default::default() };
        let files = scan(&open(&base), &cfg);
        assert_eq!(names(&files), vec!["a.md"]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn oversized_files_are_listed_but_flagged() {
        let base = tmpdir("large");
        fs::write(base.join("big.md"), vec![b'x'; 4096]).unwrap();
        fs::write(base.join("small.md"), "x").unwrap();

        let cfg = IndexConfig { max_file_size: 1024, ..Default::default() };
        let files = scan(&open(&base), &cfg);
        let big = files.iter().find(|f| f.relative.ends_with("big.md")).unwrap();
        assert!(big.too_large, "large file must be flagged");
        assert!(!big.should_index_content());
        let small = files.iter().find(|f| f.relative.ends_with("small.md")).unwrap();
        assert!(small.should_index_content());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn code_can_be_excluded_by_configuration() {
        let base = tmpdir("nocode");
        fs::write(base.join("a.md"), "x").unwrap();
        fs::write(base.join("b.rs"), "x").unwrap();
        let cfg = IndexConfig { code: false, ..Default::default() };
        let files = scan(&open(&base), &cfg);
        assert_eq!(names(&files), vec!["a.md"]);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn read_limited_truncates_instead_of_loading_everything() {
        let base = tmpdir("readlimit");
        let p = base.join("big.txt");
        fs::write(&p, vec![b'a'; 10_000]).unwrap();
        let (buf, truncated) = read_limited(&p, 100).unwrap();
        assert_eq!(buf.len(), 100);
        assert!(truncated);
        let (buf, truncated) = read_limited(&p, 100_000).unwrap();
        assert_eq!(buf.len(), 10_000);
        assert!(!truncated);
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn timestamp_formatting_matches_the_spec_example() {
        // 2026-08-14 18:42:00 UTC
        assert_eq!(format_timestamp(1_786_732_920), "2026-08-14 18:42:00");
        assert_eq!(format_timestamp(0), "1970-01-01 00:00:00");
    }
}
