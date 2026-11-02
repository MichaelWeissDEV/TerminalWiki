use std::fs;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;
use terminalwiki_core::config::WikiEntry;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::Config;
use terminalwiki_tui::app::App;
use terminalwiki_tui::event::handle_terminal_event;

struct Fixture {
    _temp: TempDir,
    config: Config,
    wikis: WikiSet,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("vault");
    fs::create_dir_all(&root).expect("vault directory");
    fs::write(
        root.join("Source.md"),
        "# Source\n\n[[Target#Target Heading!]]\n\n## Self Heading!\n",
    )
    .expect("source page");
    fs::write(
        root.join("Target.md"),
        "# Target\n\nintro\n\n## Target Heading!\n\ncontent\n",
    )
    .expect("target page");

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
    Fixture {
        _temp: temp,
        config,
        wikis,
    }
}

fn press_enter(app: &mut App<'_>) {
    handle_terminal_event(
        app,
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
    )
    .expect("enter event");
}

#[test]
fn page_link_anchor_uses_normalized_heading_anchor() {
    let fx = fixture();
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some("vault".into()),
        Some("Source".into()),
    )
    .expect("app");
    app.selected_link_idx = Some(0);

    press_enter(&mut app);

    assert_eq!(app.current_path.to_string_lossy(), "Target.md");
    let heading_line = app
        .headings
        .iter()
        .find(|heading| heading.text == "Target Heading!")
        .expect("target heading")
        .line;
    assert_eq!(app.scroll, heading_line, "must land on the target heading");
}

#[test]
fn same_page_anchor_uses_normalized_heading_anchor() {
    let fx = fixture();
    let mut app = App::new(
        &fx.wikis,
        &fx.config,
        Some("vault".into()),
        Some("Source".into()),
    )
    .expect("app");
    // This models `[[#Self Heading!]]`, which resolves as a heading target.
    app.links[0].target = terminalwiki_render::LinkTarget::Heading("self-heading".into());
    app.selected_link_idx = Some(0);

    press_enter(&mut app);

    assert_eq!(app.current_path.to_string_lossy(), "Source.md");
    let heading_line = app
        .headings
        .iter()
        .find(|heading| heading.text == "Self Heading!")
        .expect("local heading")
        .line;
    assert_eq!(app.scroll, heading_line, "must land on the local heading");
}
