//! End-to-end tests for the interactive graph view (spec items 41-45).
//!
//! These run in a single test process with `TW_CACHE_DIR` pointed at a temp
//! directory, so building an index never touches the developer's real cache.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use terminalwiki_core::config::{Config, WikiEntry};
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_tui::app::{App, Mode};

struct Fixture {
    _tmp: tempfile::TempDir,
    config: Config,
    wikis: WikiSet,
}

/// The index cache is selected by a process-global env var, so it is set once
/// for the whole test binary. Each test then uses a distinct wiki name, which
/// gives it its own index directory and its own Tantivy lock.
static CACHE: OnceLock<tempfile::TempDir> = OnceLock::new();

fn init_cache() {
    let dir = CACHE.get_or_init(|| {
        let d = tempfile::tempdir().expect("cache tempdir");
        std::env::set_var("TW_CACHE_DIR", d.path());
        d
    });
    // Guard against a caller that raced past the initialiser.
    assert!(dir.path().exists());
}

/// Creates a small linked wiki named `name` and builds its index.
fn fixture(name: &str) -> Fixture {
    init_cache();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wiki");
    fs::create_dir_all(&root).expect("wiki root");

    fs::write(
        root.join("index.md"),
        "# Home\nSee [[Heap]] and [[Stack]].\n",
    )
    .expect("index.md");
    fs::write(
        root.join("Heap.md"),
        "# Heap\nHeap notes linking [[Tcache]] and [[Stack]].\n",
    )
    .expect("Heap.md");
    fs::write(root.join("Stack.md"), "# Stack\nStack notes.\n").expect("Stack.md");
    fs::write(root.join("Tcache.md"), "# Tcache\nTcache notes.\n").expect("Tcache.md");
    // A page with a wide-character title, to exercise label measurement.
    fs::write(root.join("Wide.md"), "---\ntitle: 日本語\n---\n# 日本語\n").expect("Wide.md");

    let entry = WikiEntry {
        name: name.to_string(),
        path: root,
        mounts: Vec::new(),
    };
    let config = Config {
        wikis: vec![entry],
        default_wiki: Some(name.to_string()),
        ..Default::default()
    };

    let (wikis, errors) = WikiSet::open(&config);
    assert!(errors.is_empty(), "wiki open errors: {errors:?}");

    for w in wikis.iter() {
        terminalwiki_index::WikiIndex::build(w, &config).expect("build index");
    }

    Fixture {
        _tmp: tmp,
        config,
        wikis,
    }
}

#[test]
fn graph_view_opens_selects_navigates_and_closes() {
    const WIKI: &str = "gv_open";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");

    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.current_path, PathBuf::from("Heap.md"));

    // --- opening ---------------------------------------------------------
    app.open_graph();
    assert_eq!(app.mode, Mode::Graph, "g must enter the graph view");
    let view = app.graph_view.as_ref().expect("graph view state");
    assert_eq!(view.depth, 2, "default depth is 2 (spec item 42)");
    assert!(
        view.ordered_nodes.len() > 1,
        "Heap links to other pages, so the neighbourhood must not be a single node"
    );
    // The view opens on the page the user came from.
    assert_eq!(
        view.selected_node(),
        Some(view.sub.center),
        "selection must start on the centre node"
    );

    // --- selection is bounded -------------------------------------------
    let count = view.ordered_nodes.len();
    for _ in 0..(count + 5) {
        app.graph_select(1);
    }
    let view = app.graph_view.as_ref().unwrap();
    assert_eq!(
        view.selected,
        count - 1,
        "selection must saturate at the last node, not wrap or overflow"
    );
    for _ in 0..(count + 5) {
        app.graph_select(-1);
    }
    assert_eq!(app.graph_view.as_ref().unwrap().selected, 0);

    // --- Enter opens the selected node ----------------------------------
    // Select a node that is not the current page.
    let target = {
        let view = app.graph_view.as_ref().unwrap();
        let graph = app.graph_cache.as_ref().unwrap();
        let idx = view
            .ordered_nodes
            .iter()
            .position(|&n| {
                graph
                    .node(n)
                    .is_some_and(|x| x.id.relative.as_path() != Path::new("Heap.md"))
            })
            .expect("a non-current node exists");
        app.graph_view.as_ref().unwrap().ordered_nodes[idx]
    };
    let target_rel = app
        .graph_cache
        .as_ref()
        .unwrap()
        .node(target)
        .unwrap()
        .id
        .relative
        .clone();
    {
        let view = app.graph_view.as_mut().unwrap();
        view.selected = view
            .ordered_nodes
            .iter()
            .position(|&n| n == target)
            .unwrap();
    }

    app.graph_open_selected();
    assert_eq!(
        app.mode,
        Mode::Normal,
        "opening a node returns to the article"
    );
    assert!(
        app.graph_view.is_none(),
        "graph view is closed after opening"
    );
    assert_eq!(
        app.current_path, target_rel,
        "must have navigated to the node"
    );
    // History works like following a link.
    app.go_back();
    assert_eq!(app.current_path, PathBuf::from("Heap.md"));
}

#[test]
fn graph_depth_change_reloads_neighbourhood() {
    const WIKI: &str = "gv_depth";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");

    app.open_graph();
    let depth_2 = app.graph_view.as_ref().unwrap().ordered_nodes.len();

    app.graph_change_depth(-1);
    let view = app.graph_view.as_ref().expect("still in graph view");
    assert_eq!(view.depth, 1, "depth must decrease");
    assert!(
        view.ordered_nodes.len() <= depth_2,
        "a shallower neighbourhood cannot contain more nodes"
    );

    // Depth is clamped, never zero or negative.
    for _ in 0..5 {
        app.graph_change_depth(-1);
    }
    assert_eq!(
        app.graph_view.as_ref().unwrap().depth,
        1,
        "depth clamps at 1"
    );
}

#[test]
fn escape_closes_graph_and_keeps_article() {
    const WIKI: &str = "gv_escape";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");

    app.open_graph();
    assert_eq!(app.mode, Mode::Graph);
    app.close_graph();
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.graph_view.is_none());
    assert_eq!(
        app.current_path,
        PathBuf::from("Heap.md"),
        "closing the graph must not change the open article"
    );
}

/// `:graph` used to autocomplete and then report "Unknown command".
#[test]
fn graph_command_is_wired() {
    const WIKI: &str = "gv_cmd";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");

    app.command_input = "graph".to_string();
    app.execute_command();

    assert_eq!(app.mode, Mode::Graph, ":graph must open the graph view");
    assert!(
        !app.status_message
            .as_deref()
            .unwrap_or("")
            .contains("Unknown command"),
        "':graph' must not report itself unknown"
    );
}

/// The graph must be built once and reused, not rebuilt per keypress.
#[test]
fn graph_cache_is_reused_and_invalidatable() {
    const WIKI: &str = "gv_cache";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");

    assert!(app.graph_cache.is_none(), "graph is built lazily");
    app.load_backlinks();
    assert!(app.graph_cache.is_some(), "backlinks populate the cache");

    app.open_graph();
    assert!(app.graph_cache.is_some());

    app.invalidate_graph();
    assert!(app.graph_cache.is_none(), "invalidation drops the cache");
}
