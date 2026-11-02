//! Interactive-mode tests: tag links, the page sidebar, and page creation.

use std::fs;
use std::sync::{Mutex, OnceLock};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;
use terminalwiki_core::config::WikiEntry;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::Config;
use terminalwiki_render::LinkTarget;
use terminalwiki_tui::app::{App, Mode, PageTemplate, SidebarMode, ViewLayout};
use terminalwiki_tui::event::handle_terminal_event;

static CACHE: OnceLock<TempDir> = OnceLock::new();
/// Index files are keyed by wiki name, so fixtures sharing the cache must not
/// build in parallel.
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn lock_tests() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn init_cache() {
    CACHE.get_or_init(|| {
        let directory = tempfile::tempdir().expect("cache tempdir");
        std::env::set_var("TW_CACHE_DIR", directory.path());
        directory
    });
}

struct Fixture {
    _temp: TempDir,
    config: Config,
    wikis: WikiSet,
}

fn fixture() -> Fixture {
    init_cache();
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("vault");
    fs::create_dir_all(&root).expect("vault directory");
    fs::write(
        root.join("Source.md"),
        "# Source\n\nSee #demo in the body and follow [[Target]].\n",
    )
    .expect("source page");
    fs::write(
        root.join("Target.md"),
        "---\ntags: [demo, example]\n---\n\n# Target\n\ncontent\n",
    )
    .expect("target page");
    fs::write(root.join("Other.md"), "# Other\n\nNothing tagged here.\n").expect("other page");
    fs::write(root.join("index.md"), "# Home\n\nWelcome home.\n").expect("home page");

    let config = Config {
        default_wiki: Some("vault".to_string()),
        wikis: vec![WikiEntry {
            name: "vault".to_string(),
            path: root,
            mounts: Vec::new(),
        }],
        ..Config::default()
    };
    let (wikis, errors) = WikiSet::open(&config);
    assert!(errors.is_empty(), "wiki open errors: {errors:?}");
    for wiki in wikis.iter() {
        terminalwiki_index::WikiIndex::build(wiki, &config).expect("build index");
    }

    Fixture {
        _temp: temp,
        config,
        wikis,
    }
}

fn app<'a>(fx: &'a Fixture, page: &str) -> App<'a> {
    App::new(
        &fx.wikis,
        &fx.config,
        Some("vault".into()),
        Some(page.into()),
    )
    .expect("app")
}

fn press(app: &mut App<'_>, code: KeyCode) {
    handle_terminal_event(app, Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
        .expect("key event");
}

fn type_text(app: &mut App<'_>, text: &str) {
    for character in text.chars() {
        press(app, KeyCode::Char(character));
    }
}

#[test]
fn home_page_is_rendered_at_startup() {
    let _guard = lock_tests();
    let fx = fixture();
    let app = App::new(&fx.wikis, &fx.config, Some("vault".into()), None).expect("app");

    assert_eq!(app.current_path.to_string_lossy(), "index.md");
    let plain: String = app
        .lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|span| span.text.as_str())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        plain.contains("Welcome home."),
        "home page not rendered: {plain:?}"
    );
}

#[test]
fn body_tag_link_opens_the_finder_filtered_by_tag() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Source");
    let tag_link = app
        .links
        .iter()
        .position(|link| matches!(&link.target, LinkTarget::Tag(_)))
        .expect("body tag must be a link");
    app.selected_link_idx = Some(tag_link);

    press(&mut app, KeyCode::Enter);

    assert_eq!(app.mode, Mode::Finder);
    assert_eq!(app.finder_query, "#demo");
    assert!(
        app.finder_filtered.iter().any(|hit| hit.title == "Target"),
        "frontmatter-tagged page missing: {:?}",
        app.finder_filtered
            .iter()
            .map(|hit| &hit.title)
            .collect::<Vec<_>>()
    );
}

#[test]
fn finder_accepts_hash_and_tag_prefix_filters() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Source");

    app.finder_query = "#example".into();
    app.update_finder_filter();
    let hash_titles: Vec<&str> = app
        .finder_filtered
        .iter()
        .map(|hit| hit.title.as_str())
        .collect();
    assert_eq!(hash_titles, ["Target"]);

    app.finder_query = "tag:example".into();
    app.update_finder_filter();
    let prefix_titles: Vec<&str> = app
        .finder_filtered
        .iter()
        .map(|hit| hit.title.as_str())
        .collect();
    assert_eq!(prefix_titles, ["Target"]);
}

#[test]
fn sidebar_toggles_focuses_and_opens_pages() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Source");

    press(&mut app, KeyCode::Char('s'));
    assert!(app.sidebar_visible);
    assert_eq!(app.mode, Mode::Sidebar);
    assert_eq!(
        app.current_sidebar_index(),
        Some(app.sidebar_selected),
        "sidebar must focus the open page"
    );

    let target = app
        .fuzzy_dataset
        .items()
        .iter()
        .position(|item| item.title == "Target")
        .expect("target in sidebar");
    app.sidebar_selected = target;
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.current_path.to_string_lossy(), "Target.md");
    assert_eq!(app.mode, Mode::Normal);
    assert!(
        app.sidebar_visible,
        "sidebar stays open after opening a page"
    );

    press(&mut app, KeyCode::Char('s'));
    assert!(!app.sidebar_visible);

    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.mode, Mode::Sidebar);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert!(
        app.sidebar_visible,
        "Esc only leaves focus, it keeps the sidebar"
    );
}

#[test]
fn new_page_prompt_creates_a_todo_page_and_opens_it() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Source");

    press(&mut app, KeyCode::Char('n'));
    assert_eq!(app.mode, Mode::NewPage);
    type_text(&mut app, "Tasks");
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.new_page_template, PageTemplate::Todo);
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.current_path.to_string_lossy(), "Tasks.md");
    assert_eq!(app.mode, Mode::Normal);

    let created = fs::read_to_string(fx.wikis.require("vault").unwrap().root.join("Tasks.md"))
        .expect("created page");
    assert!(created.starts_with("# Tasks\n"), "{created}");
    assert!(created.contains("- [ ] First task"), "{created}");
    assert!(created.contains("#todo"), "{created}");

    let indexed = app
        .fuzzy_dataset
        .items()
        .iter()
        .find(|item| item.title == "Tasks")
        .expect("created page in finder index");
    assert!(
        indexed.tags.iter().any(|tag| tag == "todo"),
        "body tag must be indexed: {:?}",
        indexed.tags
    );
}

#[test]
fn new_page_never_overwrites_or_escapes_the_wiki() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Source");
    let root = fx.wikis.require("vault").unwrap().root.clone();
    let original = fs::read_to_string(root.join("Target.md")).expect("target");

    app.begin_new_page(PageTemplate::Blank);
    type_text(&mut app, "Target");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::NewPage, "failed create keeps prompt open");
    assert_eq!(
        fs::read_to_string(root.join("Target.md")).expect("target"),
        original
    );

    app.new_page_name.clear();
    type_text(&mut app, "../escape");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::NewPage);
    assert!(!root.parent().unwrap().join("escape.md").exists());
}

#[test]
fn ctrl_p_quick_bar_supports_command_prefix_and_page_creation() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Source");

    handle_terminal_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
    )
    .expect("ctrl+p");
    assert_eq!(app.mode, Mode::Finder);
    assert!(!app.finder_is_command_mode());

    type_text(&mut app, ">new CmdCreated");
    assert!(app.finder_is_command_mode());
    assert!(!app.finder_commands.is_empty());
    press(&mut app, KeyCode::Enter);

    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.current_path.to_string_lossy(), "CmdCreated.md");
}

#[test]
fn quick_note_creates_note_in_quicknotes_dir_and_triggers_editor() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Source");

    press(&mut app, KeyCode::Char('Q'));
    assert_eq!(app.mode, Mode::NewPage);
    assert_eq!(app.new_page_template, PageTemplate::QuickNote);

    // Pressing Enter even with empty name auto-creates QuickNotes/quick-note-1.md
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(
        app.current_path.to_string_lossy(),
        "QuickNotes/quick-note-1.md"
    );
    assert!(
        app.should_suspend_for_editor.is_some(),
        "Quick note must request opening external editor (nvim/vim)"
    );
}

#[test]
fn ctrl_f_in_page_search_and_ctrl_w_wiki_search_find_words_and_phrases() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Source");

    // Ctrl+F in-page search
    handle_terminal_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)),
    )
    .expect("ctrl+f");
    assert_eq!(app.mode, Mode::InPageSearch);
    type_text(&mut app, "follow");
    assert!(!app.search_matches.is_empty());
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);

    // Ctrl+W wiki-wide search for a sentence part
    handle_terminal_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL)),
    )
    .expect("ctrl+w");
    assert_eq!(app.mode, Mode::WikiSearch);
    type_text(&mut app, "Nothing tagged");
    assert_eq!(app.wiki_search_hits.len(), 1);
    assert_eq!(app.wiki_search_hits[0].title, "Other");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.current_path.to_string_lossy(), "Other.md");
}

#[test]
fn tagging_and_layout_modes_work_via_keyboard() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Other");

    // Add tag via 't'
    press(&mut app, KeyCode::Char('t'));
    assert_eq!(app.mode, Mode::TagPrompt);
    type_text(&mut app, "fresh-tag");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);

    app.finder_query = "#fresh-tag".into();
    app.update_finder_filter();
    assert!(app.finder_filtered.iter().any(|h| h.title == "Other"));

    // Toggle Zen, Wikipedia layout, and Vault Tree mode
    press(&mut app, KeyCode::Char('z'));
    assert_eq!(app.layout_mode, ViewLayout::Zen);
    press(&mut app, KeyCode::Char('w'));
    assert_eq!(app.layout_mode, ViewLayout::Wikipedia);
    press(&mut app, KeyCode::Char('v'));
    assert_eq!(app.sidebar_mode, SidebarMode::Tree);
}

#[test]
fn latex_pages_can_be_created_rendered_and_tagged_in_tui() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Source");

    app.create_page("Physics/Quantum.tex", PageTemplate::Blank)
        .expect("create .tex page");
    assert_eq!(app.current_path.to_string_lossy(), "Physics/Quantum.tex");
    assert!(
        !app.headings.is_empty(),
        "LaTeX section should produce heading"
    );

    app.add_tag_to_current_page("latex-note")
        .expect("tag .tex page");
    app.finder_query = "#latex-note".into();
    app.update_finder_filter();
    assert!(
        app.finder_filtered.iter().any(|h| h.title == "Quantum"),
        "tagged .tex page must be discoverable via #tag filter"
    );
}

#[test]
fn daily_notes_todos_bookmarks_and_global_graph_flow() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Source");

    // 1. Daily notes via 'd'
    press(&mut app, KeyCode::Char('d'));
    assert_eq!(app.mode, Mode::Normal);
    let today_path = format!(
        "Daily/{}.md",
        terminalwiki_tui::app::App::current_date_string()
    );
    assert_eq!(app.current_path.to_string_lossy(), today_path);
    assert!(app.raw_content.contains("#daily"));
    assert!(app.raw_content.contains("#todo"));
    assert!(app.raw_content.contains("- [ ]"));

    // 2. Toggle todo via 'x'
    press(&mut app, KeyCode::Char('x'));
    assert!(
        app.raw_content.contains("- [x]"),
        "Todo checkbox should toggle to - [x]"
    );
    press(&mut app, KeyCode::Char(' '));
    assert!(
        app.raw_content.contains("- [ ]"),
        "Todo checkbox should toggle back to - [ ]"
    );

    // 3. Bookmarks via 'm'
    assert!(!app.bookmarks.contains(&app.current_path));
    press(&mut app, KeyCode::Char('m'));
    assert!(app.bookmarks.contains(&app.current_path));
    press(&mut app, KeyCode::Char('m'));
    assert!(!app.bookmarks.contains(&app.current_path));
    press(&mut app, KeyCode::Char('m')); // keep bookmarked

    // 4. Link filter toggle via 'L'
    assert_eq!(app.link_filter, terminalwiki_tui::app::LinkFilter::All);
    handle_terminal_event(
        &mut app,
        Event::Key(KeyEvent::new(KeyCode::Char('L'), KeyModifiers::SHIFT)),
    )
    .expect("toggle link filter");
    assert_eq!(
        app.link_filter,
        terminalwiki_tui::app::LinkFilter::InternalOnly
    );

    // 5. Global Knowledge Graph and Enter navigation
    app.load_page("vault", "Source", true).expect("load Source");
    press(&mut app, KeyCode::Char('g'));
    assert_eq!(app.mode, Mode::Graph);
    assert!(!app.graph_view.as_ref().unwrap().is_global);

    // Toggle to global vault graph via 'a'
    press(&mut app, KeyCode::Char('a'));
    assert!(app.graph_view.as_ref().unwrap().is_global);

    // Enter opens selected note
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);

    // 6. Sidebar modes cycling
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.mode, Mode::Sidebar);
    assert_eq!(app.sidebar_mode, SidebarMode::Flat);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.sidebar_mode, SidebarMode::Tree);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.sidebar_mode, SidebarMode::Outline);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.sidebar_mode, SidebarMode::Tags);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.sidebar_mode, SidebarMode::Bookmarks);
    assert!(!app.sidebar_entries().is_empty());
}

#[test]
fn help_screen_opens_scrolls_and_closes() {
    let _guard = lock_tests();
    let fx = fixture();
    let mut app = app(&fx, "Source");

    // Open Help via '?'
    press(&mut app, KeyCode::Char('?'));
    assert_eq!(app.mode, Mode::Help);
    assert_eq!(app.scroll, 0);

    // Scroll down and up
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.scroll, 1);
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(app.scroll, 0);

    // Close via Esc
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);

    // Open Help via ':help'
    app.command_input = "help".to_string();
    app.execute_command();
    assert_eq!(app.mode, Mode::Help);

    // Close via 'q'
    press(&mut app, KeyCode::Char('q'));
    assert_eq!(app.mode, Mode::Normal);
}
