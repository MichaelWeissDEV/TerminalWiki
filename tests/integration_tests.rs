//! Comprehensive Integration Tests for TerminalWiki (spec §57-§62).

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use tempfile::tempdir;
use terminalwiki_core::config::Config;
use terminalwiki_core::wiki::Wiki;
use terminalwiki_index::{Query, TantivyStore};
use terminalwiki_render::{
    code_view::{render_code_with_options, CodeRenderOptions},
    Theme,
};

// ─── Phase 57: Test Index Lifecycle (Rebuild, Update, Modify, Delete, Rename) ──
#[test]
fn test_index_lifecycle_flow() {
    let tmp = tempdir().unwrap();
    let wiki_root = tmp.path().join("wiki");
    let cache_root = tmp.path().join("cache");
    fs::create_dir_all(&wiki_root).unwrap();
    fs::create_dir_all(&cache_root).unwrap();

    let entry = terminalwiki_core::config::WikiEntry {
        name: "testwiki".to_string(),
        path: wiki_root.clone(),
        mounts: Vec::new(),
    };
    let wiki = Wiki::open(&entry).unwrap();
    let config = Config::default();

    // 1. Create initial files
    fs::write(
        wiki_root.join("A.md"),
        "---\ntitle: Page Alpha\ntags: [alpha]\n---\n# Alpha\nContent about allocation.\n",
    )
    .unwrap();
    fs::write(
        wiki_root.join("B.md"),
        "---\ntitle: Page Beta\ntags: [beta]\n---\n# Beta\nContent about memory exploitation.\n",
    )
    .unwrap();
    fs::write(
        wiki_root.join("C.md"),
        "---\ntitle: Page Gamma\ntags: [gamma]\n---\n# Gamma\nContent about networking.\n",
    )
    .unwrap();

    // 2. Build index
    let idx_dir = cache_root.join("testwiki");
    fs::create_dir_all(&idx_dir).unwrap();

    let raw_entries = terminalwiki_index::indexer::build_all(&wiki, &config).unwrap();
    assert_eq!(raw_entries.len(), 3);
    let mut tantivy_store = TantivyStore::open_or_create(&idx_dir).unwrap();
    tantivy_store.rebuild_all(&raw_entries).unwrap();
    let states: Vec<terminalwiki_index::DocumentState> =
        raw_entries.into_iter().map(|e| e.to_state()).collect();
    terminalwiki_index::store::save_state(&idx_dir, &states).unwrap();

    let store = TantivyStore::open_reader(&idx_dir).unwrap();
    let q = Query::from_str("allocation").unwrap();
    let hits = store.search(&q, 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].title, "Page Alpha");

    // 3. Modify B.md
    fs::write(
        wiki_root.join("B.md"),
        "---\ntitle: Page Beta Modified\ntags: [beta, patched]\n---\n# Beta Modified\nPatched allocation flaw.\n",
    )
    .unwrap();

    let existing = terminalwiki_index::store::load_index(&idx_dir)
        .unwrap()
        .unwrap_or_default();
    let delta = terminalwiki_index::indexer::compute_delta(&wiki, &config, &existing).unwrap();
    assert_eq!(delta.modified.len(), 1);
    assert_eq!(delta.unchanged.len(), 2);

    let mut tantivy_store = TantivyStore::open_or_create(&idx_dir).unwrap();
    tantivy_store.apply_delta(&delta).unwrap();

    let mut new_states = delta.unchanged;
    for item in delta.added {
        new_states.push(item.to_state());
    }
    for item in delta.modified {
        new_states.push(item.to_state());
    }
    terminalwiki_index::store::save_state(&idx_dir, &new_states).unwrap();

    let store = TantivyStore::open_reader(&idx_dir).unwrap();
    let q_mod = Query::from_str("patched").unwrap();
    let hits_mod = store.search(&q_mod, 10).unwrap();
    assert_eq!(hits_mod.len(), 1);
    assert_eq!(hits_mod[0].title, "Page Beta Modified");

    // 4. Delete C.md
    fs::remove_file(wiki_root.join("C.md")).unwrap();
    let existing2 = terminalwiki_index::store::load_index(&idx_dir)
        .unwrap()
        .unwrap_or_default();
    let delta2 = terminalwiki_index::indexer::compute_delta(&wiki, &config, &existing2).unwrap();
    assert_eq!(delta2.deleted_doc_ids.len(), 1);

    let mut tantivy_store = TantivyStore::open_or_create(&idx_dir).unwrap();
    tantivy_store.apply_delta(&delta2).unwrap();

    let mut new_states2 = delta2.unchanged;
    for item in delta2.added {
        new_states2.push(item.to_state());
    }
    for item in delta2.modified {
        new_states2.push(item.to_state());
    }
    terminalwiki_index::store::save_state(&idx_dir, &new_states2).unwrap();

    let store2 = TantivyStore::open_reader(&idx_dir).unwrap();
    let q_del = Query::from_str("networking").unwrap();
    let hits_del = store2.search(&q_del, 10).unwrap();
    assert_eq!(
        hits_del.len(),
        0,
        "Deleted page C.md must no longer appear in search"
    );

    // 5. Rename A.md -> X.md
    fs::rename(wiki_root.join("A.md"), wiki_root.join("X.md")).unwrap();
    let existing3 = terminalwiki_index::store::load_index(&idx_dir)
        .unwrap()
        .unwrap_or_default();
    let delta3 = terminalwiki_index::indexer::compute_delta(&wiki, &config, &existing3).unwrap();
    assert_eq!(delta3.deleted_doc_ids.len(), 1);
    assert_eq!(delta3.added.len(), 1);

    let mut tantivy_store = TantivyStore::open_or_create(&idx_dir).unwrap();
    tantivy_store.apply_delta(&delta3).unwrap();

    let mut new_states3 = delta3.unchanged;
    for item in delta3.added {
        new_states3.push(item.to_state());
    }
    for item in delta3.modified {
        new_states3.push(item.to_state());
    }
    terminalwiki_index::store::save_state(&idx_dir, &new_states3).unwrap();

    let store3 = TantivyStore::open_reader(&idx_dir).unwrap();
    let q_ren = Query::from_str("allocation").unwrap();
    let hits_ren = store3.search(&q_ren, 10).unwrap();
    let ren_paths: Vec<&Path> = hits_ren.iter().map(|h| h.relative.as_path()).collect();
    // Step 3 rewrote B.md to "Patched allocation flaw", so "allocation" now
    // legitimately matches two documents. What the rename must guarantee is that
    // the content moved to the new path and the old doc id is gone.
    assert!(
        ren_paths.contains(&Path::new("X.md")),
        "renamed page must be searchable under its new path, got {ren_paths:?}"
    );
    assert!(
        !ren_paths.contains(&Path::new("A.md")),
        "stale pre-rename doc id must be deleted from the index, got {ren_paths:?}"
    );
}

// ─── Phase 58: Test Search Strictly Read-Only ─────────────────────────────────
#[test]
fn test_search_is_strictly_read_only() {
    let tmp = tempdir().unwrap();
    let wiki_root = tmp.path().join("wiki");
    let idx_dir = tmp.path().join("cache/testwiki");
    fs::create_dir_all(&wiki_root).unwrap();
    fs::create_dir_all(&idx_dir).unwrap();

    let entry = terminalwiki_core::config::WikiEntry {
        name: "testwiki".to_string(),
        path: wiki_root.clone(),
        mounts: Vec::new(),
    };
    let wiki = Wiki::open(&entry).unwrap();
    let config = Config::default();

    fs::write(wiki_root.join("doc.md"), "# Document\nRead only test.\n").unwrap();
    let raw_entries = terminalwiki_index::indexer::build_all(&wiki, &config).unwrap();
    let mut tantivy_store = TantivyStore::open_or_create(&idx_dir).unwrap();
    tantivy_store.rebuild_all(&raw_entries).unwrap();
    let states: Vec<terminalwiki_index::DocumentState> =
        raw_entries.into_iter().map(|e| e.to_state()).collect();
    terminalwiki_index::store::save_state(&idx_dir, &states).unwrap();

    /// Snapshots every file under the index directory as (relative path, len,
    /// mtime), so segment creation, deletion, or rewriting is detected — not
    /// just changes to state.json.
    fn snapshot_dir(dir: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            for entry in fs::read_dir(&current).expect("read index dir") {
                let entry = entry.expect("dir entry");
                let path = entry.path();
                let meta = entry.metadata().expect("entry metadata");
                if meta.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path.strip_prefix(dir).unwrap_or(&path).to_path_buf();
                    out.push((rel, meta.len(), meta.modified().expect("mtime")));
                }
            }
        }
        out.sort();
        out
    }

    let meta_before = fs::read_to_string(idx_dir.join("state.json")).unwrap();
    let files_before = snapshot_dir(&idx_dir);
    assert!(
        !files_before.is_empty(),
        "index directory must contain files before the read-only check"
    );

    // A search must never create, delete, or rebuild index data (spec Gate 2.4).
    for _ in 0..100 {
        let store = TantivyStore::open_reader(&idx_dir).unwrap();
        let q = Query::from_str("Document").unwrap();
        let hits = store.search(&q, 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    let meta_after = fs::read_to_string(idx_dir.join("state.json")).unwrap();
    assert_eq!(
        meta_before, meta_after,
        "Search operations must not modify state.json"
    );

    let files_after = snapshot_dir(&idx_dir);
    assert_eq!(
        files_before, files_after,
        "100 searches must not create, delete, or rewrite any index file"
    );
}

// ─── Phase 60: Test Home Page Priority ────────────────────────────────────────
#[test]
fn test_home_page_resolution_priority() {
    let tmp = tempdir().unwrap();
    let wiki_root = tmp.path().join("wiki");
    fs::create_dir_all(&wiki_root).unwrap();

    let entry = terminalwiki_core::config::WikiEntry {
        name: "w".to_string(),
        path: wiki_root.clone(),
        mounts: Vec::new(),
    };

    // `Wiki::open` canonicalizes its root, which on macOS resolves the temp dir
    // /var/... to /private/var/..., so expectations must be canonicalized too.
    let canonical_root = wiki_root.canonicalize().unwrap();

    // 1. Candidate fallback: index.md
    fs::write(wiki_root.join("index.md"), "# Index").unwrap();
    let wiki = Wiki::open(&entry).unwrap();
    assert_eq!(wiki.home_page(), Some(canonical_root.join("index.md")));

    // 2. Configured home override
    fs::write(
        wiki_root.join(".terminalwiki.toml"),
        "home = \"CustomHome.md\"\n",
    )
    .unwrap();
    let wiki_custom = Wiki::open(&entry).unwrap();
    fs::write(wiki_root.join("CustomHome.md"), "# Custom").unwrap();
    assert_eq!(
        wiki_custom.home_page(),
        Some(canonical_root.join("CustomHome.md"))
    );
}

// ─── Phase 61: Test Code Rendering Whitespace Preservation & Tabstops ─────────
#[test]
fn test_code_rendering_exact_whitespace() {
    let code_input = "fn main() {\n\tlet x = 1;\n    println!(\"x = {}\", x);\n}\n";
    let theme = Theme::Dark;
    let options = CodeRenderOptions {
        line_numbers: true,
        start_line: 1,
        highlight_range: None,
        tab_width: 4,
        max_width: 80,
    };

    let doc = render_code_with_options(
        code_input,
        Some("rs"),
        Path::new("src/main.rs"),
        &options,
        &theme,
    );

    assert!(doc.lines.len() >= 4); // Header + rule + 4 code lines
    let line_text: String = doc.lines[3].iter().map(|s| s.text.as_str()).collect();
    assert!(line_text.contains("    let x = 1;"));
}

// ─── Phase 62: Test Unicode Handling ──────────────────────────────────────────
#[test]
fn test_unicode_width_and_rendering() {
    use terminalwiki_core::unicode::{display_width, pad_display_width, truncate_display_width};

    let emoji = "🦀 Rust Knowledge Base";
    let japanese = "日本語のタイトル";
    let german = "Größe und Speicher";

    assert_eq!(display_width("🦀"), 2);
    // 🦀(2) + " "(1) + "Rust"(4) + " "(1) + "Knowledge"(9) + " "(1) + "Base"(4)
    assert_eq!(display_width(emoji), 22);
    assert_eq!(display_width(japanese), 16);
    assert_eq!(display_width(german), 18);

    // All-CJK input truncated with a width-1 ellipsis can only reach odd widths,
    // so the contract is "fits within the budget", not "equals the budget".
    let truncated = truncate_display_width(japanese, 8);
    assert!(
        display_width(&truncated) <= 8,
        "truncation must never exceed the budget, got {} for {truncated:?}",
        display_width(&truncated)
    );
    assert_eq!(truncated, "日本語…");

    let padded = pad_display_width("Heap", 10);
    assert_eq!(display_width(&padded), 10);
}
