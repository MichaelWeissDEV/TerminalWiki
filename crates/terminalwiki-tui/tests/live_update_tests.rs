//! Product-philosophy tests (spec Gate 19): a running TUI must reflect changes
//! made with ordinary external tools, without any TerminalWiki action.
//!
//! These drive `App` directly with change batches rather than going through the
//! terminal, so they test the update pipeline without needing a live terminal.

use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::OnceLock;

use terminalwiki_core::config::{Config, WikiEntry};
use terminalwiki_core::watch::{ChangeKind, WikiChange};
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_tui::app::{App, Mode};

static CACHE: OnceLock<tempfile::TempDir> = OnceLock::new();

fn init_cache() {
    CACHE.get_or_init(|| {
        let d = tempfile::tempdir().expect("cache tempdir");
        std::env::set_var("TW_CACHE_DIR", d.path());
        d
    });
}

struct Fx {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    config: Config,
    wikis: WikiSet,
}

fn fixture(name: &str) -> Fx {
    init_cache();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wiki");
    fs::create_dir_all(&root).expect("root");
    fs::write(root.join("index.md"), "# Home\nStart here.\n").expect("index");
    fs::write(root.join("Heap.md"), "# Heap\nAllocator notes.\n").expect("heap");

    let entry = WikiEntry {
        name: name.to_string(),
        path: root.clone(),
        mounts: Vec::new(),
    };
    let config = Config {
        wikis: vec![entry],
        default_wiki: Some(name.to_string()),
        ..Default::default()
    };
    let (wikis, errors) = WikiSet::open(&config);
    assert!(errors.is_empty(), "{errors:?}");
    for w in wikis.iter() {
        terminalwiki_index::WikiIndex::build(w, &config).expect("build");
    }
    // The canonical root is what the watcher reports paths against.
    let root = root.canonicalize().unwrap_or(root);
    Fx {
        _tmp: tmp,
        root,
        config,
        wikis,
    }
}

fn change(wiki: &str, path: PathBuf, kind: ChangeKind) -> WikiChange {
    WikiChange {
        wiki: wiki.to_string(),
        path,
        kind,
    }
}

fn finder_titles(app: &mut App) -> Vec<String> {
    app.finder_query.clear();
    app.update_finder_filter();
    app.finder_filtered
        .iter()
        .map(|h| h.title.clone())
        .collect()
}

/// Create a markdown file externally: the finder and search must both see it.
#[test]
fn external_markdown_creation_appears_in_finder_and_search() {
    const WIKI: &str = "live_create";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");

    assert!(
        !finder_titles(&mut app).iter().any(|t| t == "UAF"),
        "page must not exist yet"
    );

    // The user runs `nvim ~/wiki/UAF.md` in another terminal.
    let new_page = fx.root.join("UAF.md");
    fs::write(&new_page, "# UAF\nUse after free, dangling pointers.\n").expect("write");

    let redraw = app.apply_fs_changes(&[change(WIKI, new_page, ChangeKind::Create)]);
    assert!(redraw, "a new page must trigger a redraw");

    assert!(
        finder_titles(&mut app).iter().any(|t| t == "UAF"),
        "new page missing from finder: {:?}",
        finder_titles(&mut app)
    );

    let idx = terminalwiki_index::WikiIndex::load(WIKI).expect("load");
    let q = terminalwiki_index::Query::from_str("dangling").expect("query");
    assert_eq!(
        idx.search(&q).expect("search").len(),
        1,
        "new page content must be searchable"
    );
}

/// Code files are first-class content and must appear the same way.
#[test]
fn external_code_file_appears() {
    const WIKI: &str = "live_code";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");

    let code = fx.root.join("allocator.rs");
    fs::write(&code, "fn malloc_consolidate() { /* unlink */ }\n").expect("write");
    app.apply_fs_changes(&[change(WIKI, code, ChangeKind::Create)]);

    assert!(
        finder_titles(&mut app).iter().any(|t| t == "allocator"),
        "code file must be findable: {:?}",
        finder_titles(&mut app)
    );
}

/// Editing the open page externally must re-render it in place.
#[test]
fn editing_the_open_page_reloads_it() {
    const WIKI: &str = "live_reload";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");

    let rendered = |a: &App| -> String {
        a.lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(rendered(&app).contains("Allocator notes"));

    let page = fx.root.join("Heap.md");
    fs::write(&page, "# Heap\nCompletely rewritten body about tcache.\n").expect("rewrite");
    app.apply_fs_changes(&[change(WIKI, page, ChangeKind::Modify)]);

    let out = rendered(&app);
    assert!(out.contains("tcache"), "page not reloaded: {out}");
    assert!(!out.contains("Allocator notes"), "stale content: {out}");
    assert!(!app.page_missing);
}

/// Deleting the open page must show a notice, not crash or show stale content.
#[test]
fn deleting_the_open_page_shows_a_notice() {
    const WIKI: &str = "live_delete";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");

    let page = fx.root.join("Heap.md");
    fs::remove_file(&page).expect("remove");
    app.apply_fs_changes(&[change(WIKI, page, ChangeKind::Delete)]);

    assert!(app.page_missing, "a removed page must be reported");
    assert!(!app.should_quit, "removal must not end the session");

    // ...and it must not linger as a finder result.
    assert!(
        !finder_titles(&mut app).iter().any(|t| t == "Heap"),
        "deleted page must not remain a ghost result"
    );
}

/// Renaming the open page must follow it to its new path.
#[test]
fn renaming_the_open_page_follows_it() {
    const WIKI: &str = "live_rename";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");
    assert_eq!(app.current_path, PathBuf::from("Heap.md"));

    let from = fx.root.join("Heap.md");
    let to = fx.root.join("Heap-Exploitation.md");
    fs::rename(&from, &to).expect("rename");

    app.apply_fs_changes(&[change(WIKI, to, ChangeKind::Rename { from })]);

    assert_eq!(
        app.current_path,
        PathBuf::from("Heap-Exploitation.md"),
        "the view must follow the renamed page"
    );
    assert!(!app.page_missing);
}

/// A `git pull`-shaped bulk change must be applied as one batch.
#[test]
fn bulk_change_is_applied_in_one_pass() {
    const WIKI: &str = "live_bulk";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");

    let mut batch = Vec::new();
    for i in 0..100 {
        let p = fx.root.join(format!("pulled{i}.md"));
        fs::write(&p, format!("# Pulled {i}\nSynced content {i}.\n")).expect("write");
        batch.push(change(WIKI, p, ChangeKind::Create));
    }

    let redraw = app.apply_fs_changes(&batch);
    assert!(redraw);

    // The finder caps its result list, so query for a specific pulled page
    // rather than counting how many fit in the default page of results.
    // Titles come from frontmatter or the file stem — not the H1 heading.
    app.finder_query = "pulled73".to_string();
    app.update_finder_filter();
    assert!(
        app.finder_filtered.iter().any(|h| h.title == "pulled73"),
        "a specific bulk-added page must be findable, got {:?}",
        app.finder_filtered
            .iter()
            .map(|h| &h.title)
            .collect::<Vec<_>>()
    );

    let idx = terminalwiki_index::WikiIndex::load(WIKI).expect("load");
    assert!(
        idx.entries.len() >= 102,
        "index should hold all pulled files, got {}",
        idx.entries.len()
    );
}

/// Scroll position must be clamped, never left past the end of a shorter page.
#[test]
fn reload_clamps_scroll_position() {
    const WIKI: &str = "live_scroll";
    let fx = fixture(WIKI);
    let page = fx.root.join("Long.md");
    let long: String = (0..200).map(|i| format!("Line {i}\n")).collect();
    fs::write(&page, format!("# Long\n{long}")).expect("write");
    terminalwiki_index::WikiIndex::build(fx.wikis.iter().next().unwrap(), &fx.config)
        .expect("rebuild");

    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Long".into()),
    )
    .expect("app");
    app.scroll = 150;

    fs::write(&page, "# Long\nNow very short.\n").expect("shorten");
    app.apply_fs_changes(&[change(WIKI, page, ChangeKind::Modify)]);

    assert!(
        app.scroll < app.lines.len().max(1),
        "scroll {} must be clamped into a {}-line document",
        app.scroll,
        app.lines.len()
    );
}

/// Changes must invalidate the graph so it is rebuilt with the new links.
#[test]
fn changes_invalidate_the_graph_cache() {
    const WIKI: &str = "live_graph";
    let fx = fixture(WIKI);
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some(WIKI.into()),
        Some("Heap".into()),
    )
    .expect("app");

    app.load_backlinks();
    assert!(app.graph_cache.is_some(), "cache populated");

    let p = fx.root.join("Extra.md");
    fs::write(&p, "# Extra\nLinks to [[Heap]].\n").expect("write");
    app.apply_fs_changes(&[change(WIKI, p, ChangeKind::Create)]);

    assert!(
        app.graph_cache.is_none(),
        "graph must be invalidated so it picks up the new link"
    );

    // Rebuilt lazily, and the new backlink is visible.
    app.load_backlinks();
    assert!(
        app.backlinks.iter().any(|b| b.from_title == "Extra"),
        "new backlink missing: {:?}",
        app.backlinks
            .iter()
            .map(|b| &b.from_title)
            .collect::<Vec<_>>()
    );
    assert_eq!(app.mode, Mode::Normal);
}
