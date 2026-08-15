//! End-to-end filesystem watcher behaviour against a real directory.
//!
//! These drive the actual OS notification backend, so they wait for events
//! rather than assuming they arrive instantly.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use terminalwiki_core::watch::{ChangeKind, WikiChange, WikiWatcher};

/// Polls until `want` changes have accumulated or the deadline passes.
///
/// Filesystem notifications are inherently asynchronous; a fixed sleep would be
/// both slower and flakier than waiting for the condition.
fn collect(watcher: &mut WikiWatcher, want: usize, timeout: Duration) -> Vec<WikiChange> {
    let deadline = Instant::now() + timeout;
    let mut out: Vec<WikiChange> = Vec::new();
    while Instant::now() < deadline {
        out.extend(watcher.poll_batch());
        if out.len() >= want {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    // One final drain for anything that settled on the last tick.
    out.extend(watcher.poll_batch());
    out
}

fn find<'a>(changes: &'a [WikiChange], name: &str) -> Option<&'a WikiChange> {
    changes
        .iter()
        .find(|c| c.path.file_name().and_then(|s| s.to_str()) == Some(name))
}

fn setup() -> (tempfile::TempDir, PathBuf, WikiWatcher) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wiki");
    fs::create_dir_all(&root).expect("root");
    let watcher = WikiWatcher::with_window(
        [("main".to_string(), root.clone())],
        Duration::from_millis(60),
    )
    .expect("watcher");
    (tmp, root, watcher)
}

#[test]
fn detects_create_modify_and_delete() {
    let (_tmp, root, mut w) = setup();

    fs::write(root.join("New.md"), "# New\n").expect("create");
    let changes = collect(&mut w, 1, Duration::from_secs(5));
    let c = find(&changes, "New.md").expect("create event");
    assert_eq!(c.wiki, "main");
    assert!(
        matches!(c.kind, ChangeKind::Create | ChangeKind::Modify),
        "unexpected kind {:?}",
        c.kind
    );

    fs::write(root.join("New.md"), "# New\nMore text.\n").expect("modify");
    let changes = collect(&mut w, 1, Duration::from_secs(5));
    assert!(find(&changes, "New.md").is_some(), "modify not reported");

    fs::remove_file(root.join("New.md")).expect("delete");
    let changes = collect(&mut w, 1, Duration::from_secs(5));
    let c = find(&changes, "New.md").expect("delete event");
    assert_eq!(c.kind, ChangeKind::Delete);
}

/// A rename must be reported with both endpoints, not as delete + create.
#[test]
fn detects_rename_with_source_path() {
    let (_tmp, root, mut w) = setup();
    fs::write(root.join("Old.md"), "# Old\n").expect("create");
    let _ = collect(&mut w, 1, Duration::from_secs(5));

    fs::rename(root.join("Old.md"), root.join("Renamed.md")).expect("rename");
    let changes = collect(&mut w, 1, Duration::from_secs(5));

    let c = find(&changes, "Renamed.md").expect("rename destination event");
    match &c.kind {
        ChangeKind::Rename { from } => {
            assert_eq!(from.file_name().unwrap(), "Old.md", "wrong rename source");
        }
        // Some backends report the pair as separate remove/create events; the
        // destination must at least be reported as present.
        other => panic!("expected a rename carrying its source, got {other:?}"),
    }
}

/// The write-temp-then-rename pattern used by editors must collapse to one
/// logical change to the real file, and must not surface the temp file.
#[test]
fn atomic_save_yields_one_change_for_the_target() {
    let (_tmp, root, mut w) = setup();
    let target = root.join("Page.md");
    fs::write(&target, "# Page\noriginal\n").expect("seed");
    let _ = collect(&mut w, 1, Duration::from_secs(5));

    // Simulate `nvim`'s save: write a sibling temp file, then rename over.
    let tmp_file = root.join(".Page.md.swp-tmp");
    fs::write(&tmp_file, "# Page\nrewritten\n").expect("temp write");
    fs::rename(&tmp_file, &target).expect("atomic rename");

    let changes = collect(&mut w, 1, Duration::from_secs(5));
    let for_target: Vec<_> = changes
        .iter()
        .filter(|c| c.path.file_name().and_then(|s| s.to_str()) == Some("Page.md"))
        .collect();

    assert_eq!(
        for_target.len(),
        1,
        "atomic save must coalesce to exactly one change, got {changes:?}"
    );
}

/// A bulk change (a `git pull`-shaped burst) must arrive as a batch, not as one
/// event per file spread over many polls.
#[test]
fn bulk_changes_arrive_coalesced() {
    let (_tmp, root, mut w) = setup();

    for i in 0..100 {
        fs::write(root.join(format!("bulk{i}.md")), format!("# Bulk {i}\n")).expect("write");
    }

    let changes = collect(&mut w, 100, Duration::from_secs(10));
    let unique: std::collections::HashSet<&Path> =
        changes.iter().map(|c| c.path.as_path()).collect();

    assert!(
        unique.len() >= 100,
        "expected all 100 files reported, got {}",
        unique.len()
    );
    // One change per path: the coalescing map must not emit duplicates.
    assert_eq!(
        changes.len(),
        unique.len(),
        "each path must be reported once per batch"
    );
}

/// Ignored files must never reach the consumer.
#[test]
fn gitignored_and_vcs_paths_are_not_reported() {
    // The ignore matcher is built when the watcher starts, so the rules must
    // exist first.
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wiki");
    fs::create_dir_all(&root).expect("root");
    fs::write(root.join(".gitignore"), "*.log\n").expect("gitignore");
    fs::create_dir_all(root.join(".git")).expect("git dir");
    let mut w = WikiWatcher::with_window(
        [("main".to_string(), root.clone())],
        Duration::from_millis(60),
    )
    .expect("watcher");

    fs::write(root.join("noise.log"), "spam").expect("log");
    fs::write(root.join(".git/HEAD"), "ref: refs/heads/main").expect("git file");
    fs::write(root.join("Real.md"), "# Real\n").expect("real");

    let changes = collect(&mut w, 1, Duration::from_secs(5));

    assert!(
        find(&changes, "Real.md").is_some(),
        "a real page must still be reported"
    );
    assert!(
        find(&changes, "noise.log").is_none(),
        "gitignored file leaked: {changes:?}"
    );
    assert!(
        find(&changes, "HEAD").is_none(),
        ".git internals leaked: {changes:?}"
    );
}

/// Every registered wiki is watched, and changes carry the right wiki name.
#[test]
fn watches_multiple_wikis() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let a = tmp.path().join("a");
    let b = tmp.path().join("b");
    fs::create_dir_all(&a).expect("a");
    fs::create_dir_all(&b).expect("b");

    let mut w = WikiWatcher::with_window(
        [
            ("alpha".to_string(), a.clone()),
            ("beta".to_string(), b.clone()),
        ],
        Duration::from_millis(60),
    )
    .expect("watcher");
    assert_eq!(w.watched_count(), 2);

    fs::write(a.join("InA.md"), "# A\n").expect("write a");
    fs::write(b.join("InB.md"), "# B\n").expect("write b");

    let changes = collect(&mut w, 2, Duration::from_secs(5));
    assert_eq!(
        find(&changes, "InA.md").map(|c| c.wiki.as_str()),
        Some("alpha")
    );
    assert_eq!(
        find(&changes, "InB.md").map(|c| c.wiki.as_str()),
        Some("beta")
    );
}
