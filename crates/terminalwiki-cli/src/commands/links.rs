//! `tw links / backlinks / related PAGE` (spec §16, §17, §33).

use serde::Serialize;
use terminalwiki_core::config::Config;
use terminalwiki_core::error::{Error, Result};
use terminalwiki_core::resolve;
use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_graph::{GraphEntry, WikiGraph};

use crate::args::Args;

#[derive(Serialize)]
struct BacklinksJsonOutput {
    page: String,
    backlinks: Vec<BacklinkJsonItem>,
}

#[derive(Serialize)]
struct BacklinkJsonItem {
    wiki: String,
    from: String,
    title: String,
}

#[derive(Serialize)]
struct OutgoingLinksJsonOutput {
    page: String,
    links: Vec<OutgoingLinkJsonItem>,
}

#[derive(Serialize)]
struct OutgoingLinkJsonItem {
    target: String,
    broken: bool,
}

#[derive(Serialize)]
struct RelatedJsonOutput {
    page: String,
    related: Vec<RelatedJsonItem>,
}

#[derive(Serialize)]
struct RelatedJsonItem {
    wiki: String,
    page: String,
    title: String,
    score: f32,
    reasons: Vec<String>,
}

fn build_graph(wikis: &WikiSet, config: &Config) -> WikiGraph {
    let mut entries = Vec::new();

    for wiki in wikis.iter() {
        if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
            for e in idx.entries {
                entries.push(GraphEntry {
                    wiki: e.wiki,
                    relative: e.relative,
                    content_type: terminalwiki_core::filetype::ContentType::from(e.content_type).as_str().to_string(),
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
                let title = f
                    .relative
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                entries.push(GraphEntry {
                    wiki: wiki.name.clone(),
                    relative: f.relative,
                    content_type,
                    title,
                    tags: Vec::new(),
                    wiki_links: Vec::new(),
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
        .ok_or(Error::NoWikiConfigured)?;

    let resolution = resolve::resolve(&wikis, &start_wiki, &page, &config.index)?;
    let graph = build_graph(&wikis, &config);

    let links = graph.backlinks(&resolution.wiki, &resolution.relative);

    if args.json {
        let output = BacklinksJsonOutput {
            page: resolution.relative.to_string_lossy().to_string(),
            backlinks: links
                .into_iter()
                .map(|b| BacklinkJsonItem {
                    wiki: b.from_wiki,
                    from: b.from_relative.to_string_lossy().to_string(),
                    title: b.from_title,
                })
                .collect(),
        };
        let json_str = serde_json::to_string_pretty(&output)
            .map_err(|e| Error::other(format!("JSON error: {e}")))?;
        println!("{json_str}");
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
        .ok_or(Error::NoWikiConfigured)?;

    let resolution = resolve::resolve(&wikis, &start_wiki, &page, &config.index)?;
    let graph = build_graph(&wikis, &config);

    let edges = graph.outgoing_links(&resolution.wiki, &resolution.relative);

    if args.json {
        let output = OutgoingLinksJsonOutput {
            page: resolution.relative.to_string_lossy().to_string(),
            links: edges
                .into_iter()
                .map(|e| OutgoingLinkJsonItem {
                    target: e.target.clone(),
                    broken: e.broken,
                })
                .collect(),
        };
        let json_str = serde_json::to_string_pretty(&output)
            .map_err(|e| Error::other(format!("JSON error: {e}")))?;
        println!("{json_str}");
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
        .ok_or(Error::NoWikiConfigured)?;

    let resolution = resolve::resolve(&wikis, &start_wiki, &page, &config.index)?;
    let graph = build_graph(&wikis, &config);

    let related_pages = graph.related(&resolution.wiki, &resolution.relative, 10);

    if args.json {
        let output = RelatedJsonOutput {
            page: resolution.relative.to_string_lossy().to_string(),
            related: related_pages
                .into_iter()
                .map(|r| RelatedJsonItem {
                    wiki: r.wiki,
                    page: r.relative.to_string_lossy().to_string(),
                    title: r.title,
                    score: r.score,
                    reasons: r.reasons,
                })
                .collect(),
        };
        let json_str = serde_json::to_string_pretty(&output)
            .map_err(|e| Error::other(format!("JSON error: {e}")))?;
        println!("{json_str}");
    } else if related_pages.is_empty() {
        println!("No related pages found for '{}'.", sanitize_line(&page));
    } else {
        println!(
            "Related pages for '{}':",
            sanitize_line(&resolution.relative.to_string_lossy())
        );
        for r in related_pages {
            let reasons_str = if r.reasons.is_empty() {
                String::new()
            } else {
                format!(" ({})", r.reasons.join(", "))
            };
            println!(
                "  {}:{} · score {:.1}{}",
                sanitize_line(&r.wiki),
                sanitize_line(&r.relative.to_string_lossy()),
                r.score,
                reasons_str
            );
        }
    }

    Ok(())
}