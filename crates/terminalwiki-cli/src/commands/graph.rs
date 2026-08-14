//! `tw graph [PAGE] [--depth N] [--format dot]` (spec §18, §19, §91).

use terminalwiki_core::resolve;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};
use terminalwiki_graph::{
    export_dot, export_dot_subgraph, render_graph, GraphEntry, LayoutEngine, SubGraph, WikiGraph,
};

use crate::args::Args;

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
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let mut wiki_links = Vec::new();
                if f.content_type == terminalwiki_core::filetype::ContentType::Markdown {
                    if let Ok((bytes, _)) =
                        terminalwiki_core::scan::read_limited(&f.path, 1024 * 1024)
                    {
                        let text = String::from_utf8_lossy(&bytes);
                        for (_range, link) in terminalwiki_core::link::find_links(&text) {
                            if let terminalwiki_core::link::LinkTarget::Page { name, .. } =
                                link.target
                            {
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

pub fn graph(
    page: Option<String>,
    depth: Option<usize>,
    format: Option<String>,
    args: Args,
    config: Config,
    wikis: WikiSet,
) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let g = build_graph(&wikis, &config);
    let depth = depth.unwrap_or(2);

    let sub = if let Some(ref p) = page {
        let start_wiki = args
            .wiki
            .or_else(|| wikis.default_wiki().map(|w| w.name.clone()))
            .ok_or(Error::NoWikiConfigured)?;
        let resolution = resolve::resolve(&wikis, &start_wiki, p, &config.index)?;
        g.neighborhood(&resolution.wiki, &resolution.relative, depth)
    } else {
        // Full graph as subgraph
        SubGraph {
            nodes: (0..g.stats().total_nodes).collect(),
            edges: (0..g.stats().total_edges).collect(),
            center: 0,
        }
    };

    if format.as_deref() == Some("dot") {
        if page.is_some() {
            println!("{}", export_dot_subgraph(&g, &sub));
        } else {
            println!("{}", export_dot(&g));
        }
        return Ok(());
    }

    let width = std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(80);
    let height = 24;

    let pos = LayoutEngine::compute_layout(&g, &sub);
    let lines = render_graph(&g, &sub, &pos, width, height);

    for line in lines {
        println!("{}", line);
    }

    let stats = g.stats();
    println!(
        "\nGraph: {} nodes, {} edges (showing {} nodes)",
        stats.total_nodes,
        stats.total_edges,
        sub.nodes.len()
    );

    Ok(())
}