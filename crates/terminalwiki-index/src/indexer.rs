//! Incremental index builder using Rayon and Blake3 (spec §11-§17, §21-§23).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use rayon::prelude::*;
use terminalwiki_core::config::Config;
use terminalwiki_core::error::{Error, Result};
use terminalwiki_core::filetype::{classify, looks_binary, ContentType};
use terminalwiki_core::frontmatter::Frontmatter;
use terminalwiki_core::link::find_links;
use terminalwiki_core::sanitize::sanitize_text;
use terminalwiki_core::wiki::Wiki;

use crate::entry::{
    document_id, ContentTypeHelper, DocumentState, IndexDelta, IndexEntry, SkipReason, SkippedFile,
};

fn hash_content(content: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(content);
    *hasher.finalize().as_bytes()
}

pub fn parse_markdown(content: &str) -> (Vec<String>, String, Vec<String>) {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    let mut headings = Vec::new();
    let mut body = String::new();
    let mut links = Vec::new();

    let mut in_heading = false;
    let mut current_heading = String::new();

    let parser = Parser::new(content);

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                let trimmed = current_heading.trim();
                if !trimmed.is_empty() {
                    headings.push(trimmed.to_string());
                    current_heading.clear();
                }
            }
            Event::Text(text) => {
                let text_str = text.as_ref();
                if in_heading {
                    current_heading.push_str(text_str);
                    current_heading.push(' ');
                } else {
                    body.push_str(text_str);
                    body.push(' ');
                }

                let found_links = find_links(text_str);
                for (_, l) in found_links {
                    links.push(l.raw);
                }
            }
            Event::Code(code) => {
                let code_str = code.as_ref();
                body.push_str(code_str);
                body.push(' ');
            }
            _ => {}
        }
    }

    (headings, body, links)
}

pub fn parse_file(
    wiki_name: &str,
    wiki_root: &Path,
    path: &Path,
    max_file_size: u64,
) -> Result<Option<IndexEntry>> {
    let metadata = fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if !metadata.is_file() {
        return Ok(None);
    }

    let size = metadata.len();
    let mtime = metadata
        .modified()
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        })
        .unwrap_or(0);

    let relative = path
        .strip_prefix(wiki_root)
        .map_err(|_| Error::other("Path outside wiki root"))?
        .to_path_buf();

    let doc_id = document_id(wiki_name, &relative);

    // Read content up to max_file_size
    let is_oversized = size > max_file_size;
    let content = if !is_oversized {
        fs::read(path).map_err(|e| Error::io(path, e))?
    } else {
        Vec::new()
    };

    let is_bin = looks_binary(&content);
    let content_type = if is_bin {
        ContentType::Binary
    } else {
        classify(path, &content)
    };

    let content_hash = hash_content(&content);

    let (title, aliases, tags, headings, body_text, wiki_links) = match content_type {
        ContentType::Markdown => {
            let text = String::from_utf8_lossy(&content);
            let fm = Frontmatter::parse(&text);
            let raw_title = fm.title.unwrap_or_else(|| {
                path.file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            });

            let body_slice = if fm.body_offset < text.len() {
                &text[fm.body_offset..]
            } else {
                &text
            };
            let (h, b, l) = parse_markdown(body_slice);

            (
                sanitize_text(&raw_title),
                fm.aliases,
                fm.tags,
                h,
                sanitize_text(&b),
                l,
            )
        }
        ContentType::Text | ContentType::Code | ContentType::Latex => {
            let text = String::from_utf8_lossy(&content);
            let title = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            (
                sanitize_text(&title),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                sanitize_text(&text),
                Vec::new(),
            )
        }
        ContentType::Image | ContentType::Binary => {
            let title = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            (
                sanitize_text(&title),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                String::new(),
                Vec::new(),
            )
        }
    };

    Ok(Some(IndexEntry {
        document_id: doc_id,
        wiki: wiki_name.to_string(),
        path: path.to_path_buf(),
        relative,
        size,
        mtime,
        content_hash,
        content_type: ContentTypeHelper::from(content_type),
        title,
        aliases,
        tags,
        headings,
        body_text,
        wiki_links,
    }))
}

/// Builds all index entries from scratch for a full rebuild (spec §15).
pub fn build_all(wiki: &Wiki, config: &Config) -> Result<Vec<IndexEntry>> {
    let paths = scan_wiki_paths(&wiki.root);

    let max_size = config.index.max_file_size;
    let results: Vec<Result<Option<IndexEntry>>> = paths
        .par_iter()
        .map(|path| parse_file(&wiki.name, &wiki.root, path, max_size))
        .collect();

    let mut entries = Vec::new();
    for res in results {
        match res {
            Ok(Some(entry)) => entries.push(entry),
            Ok(None) => {}
            // A file removed between the scan and the read simply falls out of
            // this build; it must not abort the whole rebuild (spec Gate 2.2).
            Err(e) if classify_io(&e) == Some(SkipReason::Vanished) => {}
            Err(e) => return Err(e),
        }
    }

    entries.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(entries)
}

/// Classifies an indexing error as a per-file skip, or `None` if it is a real
/// failure that must abort the whole pass (e.g. a path outside the wiki root).
fn classify_io(e: &Error) -> Option<SkipReason> {
    match e {
        Error::Io { source, .. } => match source.kind() {
            io::ErrorKind::NotFound => Some(SkipReason::Vanished),
            _ => Some(SkipReason::Unreadable(source.to_string())),
        },
        _ => None,
    }
}

/// Computes an incremental delta between on-disk files and previous state (spec §13, §16).
pub fn compute_delta(
    wiki: &Wiki,
    config: &Config,
    existing: &[DocumentState],
) -> Result<IndexDelta> {
    let mut existing_map: HashMap<PathBuf, &DocumentState> = HashMap::new();
    for state in existing {
        existing_map.insert(state.relative.clone(), state);
    }

    let paths = scan_wiki_paths(&wiki.root);
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();

    let mut delta = IndexDelta::default();
    let max_size = config.index.max_file_size;

    for path in paths {
        let rel = match path.strip_prefix(&wiki.root) {
            Ok(r) => r.to_path_buf(),
            Err(_) => continue,
        };
        seen_paths.insert(rel.clone());

        let metadata = match fs::metadata(&path) {
            Ok(m) if m.is_file() => m,
            _ => continue,
        };

        let size = metadata.len();
        let mtime = metadata
            .modified()
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            })
            .unwrap_or(0);

        if let Some(prev) = existing_map.get(&rel) {
            // 1. Fast mtime & size check
            if prev.mtime == mtime && prev.size == size {
                delta.unchanged.push((*prev).clone());
                continue;
            }

            // 2. Hash check (spec §16).
            //
            // An unreadable file must never be hashed as if it were empty: that
            // would silently replace the indexed document with a 0-byte one.
            let content = if size <= max_size {
                match fs::read(&path) {
                    Ok(content) => content,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        // Scan and read are not atomic. Another process removed
                        // the file in between; drop it from this delta entirely
                        // so the next pass observes it as a normal deletion.
                        seen_paths.remove(&rel);
                        delta.skipped.push(SkippedFile {
                            relative: rel,
                            reason: SkipReason::Vanished,
                        });
                        continue;
                    }
                    Err(e) => {
                        // Permission denied, I/O error, broken symlink: keep the
                        // previously indexed content and report the file.
                        delta.unchanged.push((*prev).clone());
                        delta.skipped.push(SkippedFile {
                            relative: rel,
                            reason: SkipReason::Unreadable(e.to_string()),
                        });
                        continue;
                    }
                }
            } else {
                Vec::new()
            };
            let hash = hash_content(&content);

            if prev.content_hash == hash {
                // Content unchanged, update mtime/size metadata only
                let mut updated = (*prev).clone();
                updated.mtime = mtime;
                updated.size = size;
                delta.unchanged.push(updated);
                continue;
            }

            // Content modified -> parse
            match parse_file(&wiki.name, &wiki.root, &path, max_size) {
                Ok(Some(entry)) => delta.modified.push(entry),
                Ok(None) => {}
                Err(e) => match classify_io(&e) {
                    Some(SkipReason::Vanished) => {
                        seen_paths.remove(&rel);
                        delta.skipped.push(SkippedFile {
                            relative: rel,
                            reason: SkipReason::Vanished,
                        });
                    }
                    Some(reason) => {
                        // Retain the previous entry rather than dropping the page.
                        delta.unchanged.push((*prev).clone());
                        delta.skipped.push(SkippedFile {
                            relative: rel,
                            reason,
                        });
                    }
                    None => return Err(e),
                },
            }
        } else {
            // New file -> parse
            match parse_file(&wiki.name, &wiki.root, &path, max_size) {
                Ok(Some(entry)) => delta.added.push(entry),
                Ok(None) => {}
                Err(e) => match classify_io(&e) {
                    Some(reason) => {
                        // Never indexed before, so there is nothing to retain.
                        if reason == SkipReason::Vanished {
                            seen_paths.remove(&rel);
                        }
                        delta.skipped.push(SkippedFile {
                            relative: rel,
                            reason,
                        });
                    }
                    None => return Err(e),
                },
            }
        }
    }

    // 3. Detect deleted files
    for (rel, prev) in existing_map {
        if !seen_paths.contains(&rel) {
            delta.deleted_doc_ids.push(prev.document_id.clone());
        }
    }

    Ok(delta)
}

fn scan_wiki_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .build();

    for entry in walker.flatten() {
        if let Some(ft) = entry.file_type() {
            if ft.is_file() {
                paths.push(entry.into_path());
            }
        }
    }
    paths
}

#[cfg(test)]
mod io_resilience_tests {
    use super::*;
    use terminalwiki_core::config::WikiEntry;

    /// Builds a wiki with a single page and returns (tempdir, wiki, config).
    fn fixture() -> (tempfile::TempDir, Wiki, Config) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("wiki");
        fs::create_dir_all(&root).expect("create wiki root");
        fs::write(root.join("Page.md"), "# Page\nOriginal content.\n").expect("write page");

        let entry = WikiEntry {
            name: "w".to_string(),
            path: root,
            mounts: Vec::new(),
        };
        let wiki = Wiki::open(&entry).expect("open wiki");
        (tmp, wiki, Config::default())
    }

    /// An unreadable file must keep its indexed content and be reported —
    /// never be hashed as if it were 0 bytes (spec Gate 2.1).
    #[cfg(unix)]
    #[test]
    fn unreadable_file_is_reported_and_retains_previous_content() {
        use std::os::unix::fs::PermissionsExt;

        let (_tmp, wiki, config) = fixture();
        let page = wiki.root.join("Page.md");

        let initial = build_all(&wiki, &config).expect("initial build");
        let states: Vec<DocumentState> = initial.into_iter().map(|e| e.to_state()).collect();
        assert_eq!(states.len(), 1);
        let original_hash = states[0].content_hash;

        // Change size+mtime so the fast path cannot short-circuit, then make it
        // unreadable so the content read fails with PermissionDenied.
        fs::write(&page, "# Page\nRewritten, longer content here.\n").expect("rewrite");
        fs::set_permissions(&page, fs::Permissions::from_mode(0o000)).expect("chmod");

        let delta = compute_delta(&wiki, &config, &states).expect("delta must not abort");

        // Restore permissions before any assertion can abort the test.
        let _ = fs::set_permissions(&page, fs::Permissions::from_mode(0o644));

        assert_eq!(delta.skipped.len(), 1, "unreadable file must be reported");
        assert!(
            matches!(delta.skipped[0].reason, SkipReason::Unreadable(_)),
            "expected Unreadable, got {:?}",
            delta.skipped[0].reason
        );
        assert!(
            delta.deleted_doc_ids.is_empty(),
            "an unreadable file must not be deleted from the index"
        );
        assert_eq!(
            delta.unchanged.len(),
            1,
            "previously indexed content must be retained"
        );
        assert_eq!(
            delta.unchanged[0].content_hash, original_hash,
            "retained entry must keep its original hash, not the hash of empty content"
        );
        assert!(delta.modified.is_empty());
    }

    /// Root can read anything, which would make the permission test vacuous.
    #[cfg(unix)]
    #[test]
    fn unreadable_test_is_meaningful_for_this_user() {
        assert_ne!(
            unsafe { libc_geteuid() },
            0,
            "running as root makes the permission-denied test vacuous"
        );
    }

    #[cfg(unix)]
    unsafe fn libc_geteuid() -> u32 {
        extern "C" {
            fn geteuid() -> u32;
        }
        geteuid()
    }

    /// A file removed between scan and read must fall out of the delta cleanly
    /// rather than crashing or being indexed as empty (spec Gate 2.2).
    #[test]
    fn vanished_file_falls_out_of_delta() {
        let (_tmp, wiki, config) = fixture();

        // A path that the scan will not find at all: simulate the race by
        // computing a delta whose previous state references a now-absent file.
        let missing = DocumentState {
            document_id: document_id("w", Path::new("Gone.md")),
            wiki: "w".to_string(),
            path: wiki.root.join("Gone.md"),
            relative: PathBuf::from("Gone.md"),
            size: 10,
            mtime: 0,
            content_hash: [0u8; 32],
            content_type: ContentTypeHelper::Markdown,
            title: "Gone".to_string(),
            aliases: Vec::new(),
            tags: Vec::new(),
            headings: Vec::new(),
            wiki_links: Vec::new(),
        };

        let delta = compute_delta(&wiki, &config, &[missing]).expect("delta must not abort");
        assert_eq!(
            delta.deleted_doc_ids.len(),
            1,
            "a file absent from disk must be deleted from the index"
        );
    }
}
