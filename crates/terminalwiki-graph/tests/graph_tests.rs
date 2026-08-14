use terminalwiki_graph::{
    export_dot, export_dot_subgraph, render_graph, GraphEntry, LayoutEngine, WikiGraph,
};
use std::path::PathBuf;

fn sample_entries() -> Vec<GraphEntry> {
    vec![
        GraphEntry {
            wiki: "main".into(),
            relative: PathBuf::from("index.md"),
            content_type: "markdown".into(),
            title: "Home".into(),
            tags: vec!["home".into()],
            wiki_links: vec!["about".into(), "broken".into()],
            image_links: vec![],
        },
        GraphEntry {
            wiki: "main".into(),
            relative: PathBuf::from("about.md"),
            content_type: "markdown".into(),
            title: "About".into(),
            tags: vec!["info".into(), "home".into()],
            wiki_links: vec!["index".into()],
            image_links: vec!["logo.png".into()],
        },
        GraphEntry {
            wiki: "main".into(),
            relative: PathBuf::from("logo.png"),
            content_type: "image".into(),
            title: "Logo".into(),
            tags: vec![],
            wiki_links: vec![],
            image_links: vec![],
        },
    ]
}

#[test]
fn test_build_and_links() {
    let entries = sample_entries();
    let graph = WikiGraph::from_entries(&entries);

    // Stats
    let stats = graph.stats();
    assert_eq!(stats.total_nodes, 3);
    assert_eq!(stats.total_edges, 4); // index->about, index->broken, about->index, about->logo
    assert_eq!(stats.broken_links, 1);

    // Outgoing
    let out = graph.outgoing_links("main", &PathBuf::from("index.md"));
    assert_eq!(out.len(), 2);
    
    // Backlinks
    let backs = graph.backlinks("main", &PathBuf::from("about.md"));
    assert_eq!(backs.len(), 1);
    assert_eq!(backs[0].from_relative, PathBuf::from("index.md"));

    // Broken links
    let broken = graph.broken_links();
    assert_eq!(broken.len(), 1);
    assert_eq!(broken[0].target, "broken");
}

#[test]
fn test_related() {
    let entries = sample_entries();
    let graph = WikiGraph::from_entries(&entries);

    let rel = graph.related("main", &PathBuf::from("index.md"), 10);
    assert!(!rel.is_empty());
    assert_eq!(rel[0].relative, PathBuf::from("about.md"));
    // direct + backlink + shared tag = 10 + 8 + 5 = 23 (approx)
    assert!(rel[0].score > 15.0);
}

#[test]
fn test_neighborhood_cycles() {
    let entries = vec![
        GraphEntry {
            wiki: "main".into(),
            relative: PathBuf::from("a.md"),
            content_type: "markdown".into(),
            title: "A".into(),
            tags: vec![],
            wiki_links: vec!["b".into()],
            image_links: vec![],
        },
        GraphEntry {
            wiki: "main".into(),
            relative: PathBuf::from("b.md"),
            content_type: "markdown".into(),
            title: "B".into(),
            tags: vec![],
            wiki_links: vec!["a".into()],
            image_links: vec![],
        },
    ];
    let graph = WikiGraph::from_entries(&entries);
    let sub = graph.neighborhood("main", &PathBuf::from("a.md"), 5);
    assert_eq!(sub.nodes.len(), 2);
}

#[test]
fn test_layout_edge_cases() {
    let graph = WikiGraph::from_entries(&[]);
    let sub = graph.neighborhood("main", &PathBuf::from("nonexistent.md"), 1);
    let pos = LayoutEngine::compute_layout(&graph, &sub);
    assert!(pos.is_empty());

    let entries = vec![
        GraphEntry {
            wiki: "main".into(),
            relative: PathBuf::from("a.md"),
            content_type: "markdown".into(),
            title: "A".into(),
            tags: vec![],
            wiki_links: vec![],
            image_links: vec![],
        }
    ];
    let graph = WikiGraph::from_entries(&entries);
    let sub = graph.neighborhood("main", &PathBuf::from("a.md"), 1);
    let pos = LayoutEngine::compute_layout(&graph, &sub);
    assert_eq!(pos.len(), 1);
}

#[test]
fn test_export_dot() {
    let entries = sample_entries();
    let graph = WikiGraph::from_entries(&entries);
    let dot = export_dot(&graph);
    assert!(dot.contains("digraph G {"));
    assert!(dot.contains("index.md"));
    assert!(dot.contains("about.md"));

    let sub = graph.neighborhood("main", &PathBuf::from("index.md"), 1);
    let sub_dot = export_dot_subgraph(&graph, &sub);
    assert!(sub_dot.contains("digraph G {"));
}

#[test]
fn test_render_ascii() {
    let entries = sample_entries();
    let graph = WikiGraph::from_entries(&entries);
    let sub = graph.neighborhood("main", &PathBuf::from("index.md"), 2);
    let pos = LayoutEngine::compute_layout(&graph, &sub);
    
    let grid = render_graph(&graph, &sub, &pos, 40, 20);
    assert_eq!(grid.len(), 20);
    assert_eq!(grid[0].len(), 40);
}
