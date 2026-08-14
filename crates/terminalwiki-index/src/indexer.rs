//! Incremental index builder using Rayon and Blake3 (spec §15, §16).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use rayon::prelude::*;
use terminalwiki_core::config::Config;
use terminalwiki_core::error::{Error, Result};
use terminalwiki_core::filetype::{classify, ContentType};
use terminalwiki_core::frontmatter::Frontmatter;
use terminalwiki_core::link::find_links;
use terminalwiki_core::sanitize::sanitize_text;
use terminalwiki_core::wiki::Wiki;

use crate::entry::{ContentTypeHelper, IndexEntry};

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

pub fn index_file(
    wiki_name: &str,
    wiki_root: &Path,
    path: &Path,
    existing: Option<&IndexEntry>,
) -> Result<Option<IndexEntry>> {
    let metadata = fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if !metadata.is_file() {
        return Ok(None);
    }

    let size = metadata.len();
    let mtime = metadata
        .modified()
        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
        .unwrap_or(0);

    let relative = path
        .strip_prefix(wiki_root)
        .map_err(|_| Error::other("Path outside wiki root"))?
        .to_path_buf();

    // Incremental skip check: if size & mtime match, keep existing
    if let Some(entry) = existing {
        if entry.size == size && entry.mtime == mtime {
            return Ok(Some(entry.clone()));
        }
    }

    // Read with size ceiling (2 MB for full text parsing)
    let max_read = 2 * 1024 * 1024;
    let content = if size <= max_read {
        fs::read(path).map_err(|e| Error::io(path, e))?
    } else {
        Vec::new()
    };

    let content_type = classify(path, &content);
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
            (sanitize_text(&title), Vec::new(), Vec::new(), Vec::new(), sanitize_text(&text), Vec::new())
        }
        ContentType::Image | ContentType::Binary => {
            let title = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            (sanitize_text(&title), Vec::new(), Vec::new(), Vec::new(), String::new(), Vec::new())
        }
    };

    Ok(Some(IndexEntry {
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

pub fn update_index(
    wiki: &Wiki,
    _config: &Config,
    existing_entries: Option<Vec<IndexEntry>>,
) -> Result<Vec<IndexEntry>> {
    let mut existing_map: HashMap<PathBuf, IndexEntry> = HashMap::new();
    if let Some(entries) = existing_entries {
        for entry in entries {
            existing_map.insert(entry.relative.clone(), entry);
        }
    }

    let mut paths = Vec::new();
    let walker = WalkBuilder::new(&wiki.root)
        .hidden(false)
        .git_ignore(true)
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                if let Some(ft) = entry.file_type() {
                    if ft.is_file() {
                        paths.push(entry.into_path());
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning while scanning wiki: {e}");
            }
        }
    }

    let results: Vec<Result<Option<IndexEntry>>> = paths
        .par_iter()
        .map(|path| {
            let rel = path
                .strip_prefix(&wiki.root)
                .unwrap_or(path)
                .to_path_buf();
            let existing = existing_map.get(&rel);
            index_file(&wiki.name, &wiki.root, path, existing)
        })
        .collect();

    let mut new_entries = Vec::new();
    for res in results {
        if let Some(entry) = res? {
            new_entries.push(entry);
        }
    }

    new_entries.sort_by(|a, b| a.relative.cmp(&b.relative));
    Ok(new_entries)
}
