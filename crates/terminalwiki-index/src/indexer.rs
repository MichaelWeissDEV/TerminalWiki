//! Incremental index builder using Rayon and Blake3 (spec §11-§17, §21-§23).

use std::collections::{HashMap, HashSet};
use std::fs;
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

use crate::entry::{document_id, ContentTypeHelper, DocumentState, IndexDelta, IndexEntry};

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
        if let Some(entry) = res? {
            entries.push(entry);
        }
    }

    entries.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(entries)
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

            // 2. Hash check (spec §16)
            let content = if size <= max_size {
                fs::read(&path).unwrap_or_default()
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
            if let Some(entry) = parse_file(&wiki.name, &wiki.root, &path, max_size)? {
                delta.modified.push(entry);
            }
        } else {
            // New file -> parse
            if let Some(entry) = parse_file(&wiki.name, &wiki.root, &path, max_size)? {
                delta.added.push(entry);
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
