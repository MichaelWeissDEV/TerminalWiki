use std::collections::HashMap;
use std::fs;
use std::path::Path;

use rayon::prelude::*;
use ignore::WalkBuilder;

use terminalwiki_core::config::Config;
use terminalwiki_core::error::{Error, Result};
use terminalwiki_core::filetype::{classify, ContentType};
use terminalwiki_core::frontmatter::Frontmatter;
use terminalwiki_core::link::find_links;
use terminalwiki_core::sanitize;
use terminalwiki_core::wiki::Wiki;

use crate::entry::IndexEntry;

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
            Event::End(TagEnd::Heading { .. }) => {
                in_heading = false;
                if !current_heading.trim().is_empty() {
                    headings.push(current_heading.trim().to_string());
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
            Event::Code(text) => {
                body.push_str(text.as_ref());
                body.push(' ');
            }
            _ => {}
        }
    }

    (headings, sanitize::sanitize_text(&body), links)
}

pub fn index_file(
    wiki: &Wiki,
    path: &Path,
    relative: &Path,
    size: u64,
    mtime: u64,
) -> Result<IndexEntry> {
    let content_bytes = fs::read(path).map_err(|e| Error::Io {
        path: Some(path.to_path_buf()),
        source: e,
    })?;
    
    let content_type = classify(path, &content_bytes);
    let content_hash = hash_content(&content_bytes);

    let too_large = size > 2 * 1024 * 1024; // 2MB arbitrary limit for too large text indexing

    let (title, aliases, tags, headings, body_text, wiki_links) = match content_type {
        ContentType::Markdown if !too_large => {
            if let Ok(text) = String::from_utf8(content_bytes) {
                let fm = Frontmatter::parse(&text);
                let text_without_fm = &text[fm.body_offset..];
                let (headings, body, links) = parse_markdown(text_without_fm);
                (
                    fm.title.unwrap_or_else(|| {
                        relative
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string()
                    }),
                    fm.aliases,
                    fm.tags,
                    headings,
                    body,
                    links,
                )
            } else {
                (
                    relative.to_string_lossy().to_string(),
                    vec![],
                    vec![],
                    vec![],
                    String::new(),
                    vec![],
                )
            }
        }
        ContentType::Text | ContentType::Code if !too_large => {
            if let Ok(text) = String::from_utf8(content_bytes) {
                (
                    relative.to_string_lossy().to_string(),
                    vec![],
                    vec![],
                    vec![],
                    sanitize::sanitize_text(&text),
                    vec![],
                )
            } else {
                (
                    relative.to_string_lossy().to_string(),
                    vec![],
                    vec![],
                    vec![],
                    String::new(),
                    vec![],
                )
            }
        }
        _ => (
            relative.to_string_lossy().to_string(),
            vec![],
            vec![],
            vec![],
            String::new(),
            vec![],
        ),
    };

    Ok(IndexEntry {
        wiki: wiki.name.clone(),
        path: path.to_path_buf(),
        relative: relative.to_path_buf(),
        size,
        mtime,
        content_hash,
        content_type,
        title,
        aliases,
        tags,
        headings,
        body_text,
        wiki_links,
    })
}

pub fn update_index(
    wiki: &Wiki,
    _config: &Config,
    existing: Option<Vec<IndexEntry>>,
) -> Result<Vec<IndexEntry>> {
    let mut old_entries = HashMap::new();
    if let Some(entries) = existing {
        for e in entries {
            old_entries.insert(e.path.clone(), e);
        }
    }

    let mut paths_to_process = Vec::new();

    let walker = WalkBuilder::new(&wiki.root)
        .hidden(true)
        .git_ignore(true)
        .build();

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.is_file() {
            let md = entry.metadata().ok();
            let size = md.as_ref().map(|m| m.len()).unwrap_or(0);
            let mtime = md
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            paths_to_process.push((path.to_path_buf(), size, mtime));
        }
    }

    // Parallel process
    let new_entries: Vec<IndexEntry> = paths_to_process
        .into_par_iter()
        .filter_map(|(path, size, mtime)| {
            if let Some(old) = old_entries.get(&path) {
                if old.mtime == mtime && old.size == size {
                    return Some(old.clone());
                }
            }

            let rel = path.strip_prefix(&wiki.root).unwrap_or(&path);
            index_file(wiki, &path, rel, size, mtime).ok()
        })
        .collect();

    Ok(new_entries)
}
