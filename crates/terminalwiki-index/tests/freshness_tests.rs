//! Automatic index freshness: the wiki directory is the source of truth, so
//! files created, changed or removed with any external tool must be visible
//! without a manual `tw index update` (spec Gate 1).

use std::fs;
use std::str::FromStr;
use std::sync::OnceLock;

use terminalwiki_core::config::{Config, WikiEntry};
use terminalwiki_core::wiki::Wiki;
use terminalwiki_index::{Query, Reconciliation, WikiIndex};

/// The index cache location is a process-global env var, so it is set once and
/// each test uses a distinct wiki name to get its own index directory.
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
    wiki: Wiki,
    config: Config,
}

fn fixture(name: &str) -> Fx {
    init_cache();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wiki");
    fs::create_dir_all(&root).expect("root");
    fs::write(root.join("index.md"), "# Home\nWelcome.\n").expect("index.md");

    let entry = WikiEntry {
        name: name.to_string(),
        path: root,
        mounts: Vec::new(),
    };
    let wiki = Wiki::open(&entry).expect("open wiki");
    let config = Config {
        wikis: vec![entry],
        default_wiki: Some(name.to_string()),
        ..Default::default()
    };
    Fx {
        _tmp: tmp,
        wiki,
        config,
    }
}

fn search_count(idx: &WikiIndex, q: &str) -> usize {
    let query = Query::from_str(q).expect("query");
    idx.search(&query).map(|h| h.len()).unwrap_or(0)
}

/// With no index at all, opening must build one rather than report nothing.
#[test]
fn missing_index_is_built_on_open() {
    let fx = fixture("fr_missing");
    let (idx, recon) = WikiIndex::open(&fx.wiki, &fx.config).expect("open");
    assert!(
        matches!(recon, Reconciliation::Rebuilt(_)),
        "expected a rebuild, got {recon:?}"
    );
    assert_eq!(idx.entries.len(), 1);
}

/// The headline workflow: create a file with an external tool, then find it.
#[test]
fn externally_created_file_is_visible_without_manual_update() {
    let fx = fixture("fr_create");
    let (_idx, _) = WikiIndex::open(&fx.wiki, &fx.config).expect("initial open");

    fs::write(
        fx.wiki.root.join("Photosynthesis.md"),
        "# Photosynthesis\nChloroplasts convert light.\n",
    )
    .expect("write");

    let (idx, recon) = WikiIndex::open(&fx.wiki, &fx.config).expect("reopen");
    assert_eq!(recon, Reconciliation::Updated { changed: 1 });
    assert_eq!(
        search_count(&idx, "chloroplasts"),
        1,
        "new file not searchable"
    );
}

/// An unchanged wiki must report Current and write nothing.
#[test]
fn unchanged_wiki_reports_already_current() {
    let fx = fixture("fr_current");
    WikiIndex::open(&fx.wiki, &fx.config).expect("initial");

    let (_idx, recon) = WikiIndex::open(&fx.wiki, &fx.config).expect("second");
    assert_eq!(
        recon,
        Reconciliation::AlreadyCurrent,
        "an unchanged wiki must not be rewritten"
    );
}

/// External modification must replace the old content, not add to it.
#[test]
fn externally_modified_file_is_reindexed() {
    let fx = fixture("fr_modify");
    let page = fx.wiki.root.join("Topic.md");
    fs::write(&page, "# Topic\nOriginal mentions photosynthesis.\n").expect("write");
    WikiIndex::open(&fx.wiki, &fx.config).expect("initial");

    fs::write(
        &page,
        "# Topic\nRewritten to mention mitochondria instead.\n",
    )
    .expect("rewrite");
    let (idx, _) = WikiIndex::open(&fx.wiki, &fx.config).expect("reopen");

    assert_eq!(search_count(&idx, "mitochondria"), 1, "new content missing");
    assert_eq!(
        search_count(&idx, "photosynthesis"),
        0,
        "old content must not linger as a ghost result"
    );
}

/// External deletion must not leave a ghost result.
#[test]
fn externally_deleted_file_disappears() {
    let fx = fixture("fr_delete");
    let page = fx.wiki.root.join("Doomed.md");
    fs::write(&page, "# Doomed\nEphemeral content here.\n").expect("write");
    let (idx, _) = WikiIndex::open(&fx.wiki, &fx.config).expect("initial");
    assert_eq!(search_count(&idx, "ephemeral"), 1);

    fs::remove_file(&page).expect("remove");
    let (idx, _) = WikiIndex::open(&fx.wiki, &fx.config).expect("reopen");
    assert_eq!(
        search_count(&idx, "ephemeral"),
        0,
        "deleted page must not remain searchable"
    );
}

/// `auto_update = false` must leave the stored index untouched.
#[test]
fn auto_update_disabled_does_not_reconcile() {
    let fx = fixture("fr_manual");
    WikiIndex::open(&fx.wiki, &fx.config).expect("initial build");

    let mut config = fx.config.clone();
    config.index.auto_update = false;

    fs::write(
        fx.wiki.root.join("Late.md"),
        "# Late\nUnique marker here.\n",
    )
    .expect("write");

    let (idx, recon) = WikiIndex::open(&fx.wiki, &config).expect("open");
    assert_eq!(recon, Reconciliation::AutoUpdateDisabled);
    assert_eq!(
        search_count(&idx, "unique"),
        0,
        "with auto_update off the new file must stay invisible"
    );

    // ...and turning it back on picks the file up.
    let (idx, _) = WikiIndex::open(&fx.wiki, &fx.config).expect("reopen with auto");
    assert_eq!(search_count(&idx, "unique"), 1);
}

/// A schema change must be a reported rebuild, not a silent per-document update.
#[test]
fn schema_mismatch_is_reported_as_rebuild() {
    let fx = fixture("fr_schema");
    WikiIndex::open(&fx.wiki, &fx.config).expect("initial");

    // Rewrite persisted state with an impossible schema version.
    let dir = terminalwiki_core::paths::index_dir_for("fr_schema").expect("index dir");
    let state_path = dir.join("state.json");
    let raw = fs::read_to_string(&state_path).expect("read state");
    let mut json: serde_json::Value = serde_json::from_str(&raw).expect("parse");
    json["schema_version"] = serde_json::json!(9999);
    fs::write(&state_path, serde_json::to_string(&json).unwrap()).expect("write state");

    let mut messages = Vec::new();
    let (_idx, recon) =
        WikiIndex::open_with(&fx.wiki, &fx.config, |m| messages.push(m.to_string())).expect("open");

    assert!(
        matches!(
            recon,
            Reconciliation::Rebuilt(terminalwiki_index::RebuildReason::SchemaChanged { .. })
        ),
        "expected a schema rebuild, got {recon:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("schema changed")),
        "the user must be told before a full rebuild, got {messages:?}"
    );
}
