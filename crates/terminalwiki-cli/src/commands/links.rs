//! `tw links PAGE`, `tw backlinks PAGE`, `tw related PAGE` (spec §16, §17).

use terminalwiki_core::resolve;
use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};
use terminalwiki_graph::{GraphEntry, WikiGraph};

use crate::args::Args;

fn build_graph(wikis: &WikiSet, config: &Config) -> WikiGraph {
    let mut entries = Vec::new();

    for wiki in wikis.iter() {
        if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
            for e in idx.entries {
                entries.push(GraphEntry {
                    wiki: e.wiki,
                    relative: e.relative,
                    content_type: e.content_type.as_str().to_string(),
                    title: e.title,
                    tags: e.tags,
                    wiki_links: e.wiki_links,
                    image_links: Vec::new(),
                });
            }
        } else {
            let files = terminalwiki_core::scan::scan(wiki, &config.index);
            for f in files {
                let content_type = f.content_type.as_str().to_string();
                let title = f.relative.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                let mut wiki_links = Vec::new();
                if f.content_type == terminalwiki_core::filetype::ContentType::Markdown {
                    if let Ok((bytes, _)) = terminalwiki_core::scan::read_limited(&f.path, 1024 * 1024) {
                        let text = String::from_utf8_lossy(&bytes);
                        for (_range, link) in terminalwiki_core::link::find_links(&text) {
                            if let terminalwiki_core::link::LinkTarget::Page { name, .. } = link.target {
                                wiki_links.push(name);
                            }
                        }
                    }
                }
                entries.push(GraphEntry {
                    wiki: wiki.name.clone(),
                    relative: f.relative,
                    content_type,
                    title,
                    tags: Vec::new(),
                    wiki_links,
                    image_links: Vec::new(),
                });
            }
        }
    }

    WikiGraph::from_entries(&entries)
}

pub fn backlinks(page: String, args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let start_wiki = args
        .wiki
        .or_else(|| wikis.default_wiki().map(|w| w.name.clone()))
        .ok_or_else(|| Error::NoWikiConfigured)?;

    let resolution = resolve::resolve(&wikis, &start_wiki, &page, &config.index)?;
    let graph = build_graph(&wikis, &config);

    let links = graph.backlinks(&resolution.wiki, &resolution.relative);

    if args.json {
        println!(
            "{{\"page\":\"{}\",\"backlinks\":[{}]}}",
            sanitize_line(&resolution.relative.to_string_lossy()),
            links
                .iter()
                .map(|b| format!(
                    "{{\"wiki\":\"{}\",\"from\":\"{}\",\"title\":\"{}\"}}",
                    sanitize_line(&b.from_wiki),
                    sanitize_line(&b.from_relative.to_string_lossy()),
                    sanitize_line(&b.from_title)
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
    } else if links.is_empty() {
        println!("No backlinks found for '{}'.", sanitize_line(&page));
    } else {
        println!(
            "Pages linking to '{}' ({}):",
            sanitize_line(&resolution.relative.to_string_lossy()),
            links.len()
        );
        for b in links {
            println!(
                "  {}:{} ({})",
                sanitize_line(&b.from_wiki),
                sanitize_line(&b.from_relative.to_string_lossy()),
                sanitize_line(&b.from_title)
            );
        }
    }

    Ok(())
}

pub fn links(page: String, args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let start_wiki = args
        .wiki
        .or_else(|| wikis.default_wiki().map(|w| w.name.clone()))
        .ok_or_else(|| Error::NoWikiConfigured)?;

    let resolution = resolve::resolve(&wikis, &start_wiki, &page, &config.index)?;
    let graph = build_graph(&wikis, &config);

    let edges = graph.outgoing_links(&resolution.wiki, &resolution.relative);

    if args.json {
        println!(
            "{{\"page\":\"{}\",\"links\":[{}]}}",
            sanitize_line(&resolution.relative.to_string_lossy()),
            edges
                .iter()
                .map(|e| format!(
                    "{{\"target\":\"{}\",\"broken\":{}}}",
                    sanitize_line(&e.target),
                    e.broken
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
    } else if edges.is_empty() {
        println!("No outgoing links found in '{}'.", sanitize_line(&page));
    } else {
        println!(
            "Outgoing links from '{}' ({}):",
            sanitize_line(&resolution.relative.to_string_lossy()),
            edges.len()
        );
        for e in edges {
            let status = if e.broken { " [broken]" } else { "" };
            println!("  [[{}]]{}", sanitize_line(&e.target), status);
        }
    }

    Ok(())
}

pub fn related(page: String, args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let start_wiki = args
        .wiki
        .or_else(|| wikis.default_wiki().map(|w| w.name.clone()))
        .ok_or_else(|| Error::NoWikiConfigured)?;

    let resolution = resolve::resolve(&wikis, &start_wiki, &page, &config.index)?;
    let graph = build_graph(&wikis, &config);

    let related_pages = graph.related(&resolution.wiki, &resolution.relative, 10);

    if args.json {
        println!(
            "{{\"page\":\"{}\",\"related\":[{}]}}",
            sanitize_line(&resolution.relative.to_string_lossy()),
            related_pages
                .iter()
                .map(|r| format!(
                    "{{\"wiki\":\"{}\",\"path\":\"{}\",\"title\":\"{}\",\"score\":{:.1}}}",
                    sanitize_line(&r.wiki),
                    sanitize_line(&r.relative.to_string_lossy()),
                    sanitize_line(&r.title),
                    r.score
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
    } else if related_pages.is_empty() {
        println!("No related pages found for '{}'.", sanitize_line(&page));
    } else {
        println!(
            "Related pages for '{}':",
            sanitize_line(&resolution.relative.to_string_lossy())
        );
        for r in related_pages {
            println!(
                "  {}:{} — {} (score: {:.1})",
                sanitize_line(&r.wiki),
                sanitize_line(&r.relative.to_string_lossy()),
                sanitize_line(&r.title),
                r.score
            );
        }
    }

    Ok(())
}