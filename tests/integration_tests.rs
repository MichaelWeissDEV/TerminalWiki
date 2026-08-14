//! Comprehensive Integration Tests for TerminalWiki (spec §57-§62).

use std::fs;
use std::path::Path;
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

    let entries = terminalwiki_index::indexer::update_index(&wiki, &config, None).unwrap();
    assert_eq!(entries.len(), 3);
    terminalwiki_index::store::save_index(&idx_dir, &entries).unwrap();

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

    let existing = terminalwiki_index::store::load_index(&idx_dir).unwrap();
    let updated_entries =
        terminalwiki_index::indexer::update_index(&wiki, &config, existing).unwrap();
    terminalwiki_index::store::save_index(&idx_dir, &updated_entries).unwrap();

    let store = TantivyStore::open_reader(&idx_dir).unwrap();
    let q_mod = Query::from_str("patched").unwrap();
    let hits_mod = store.search(&q_mod, 10).unwrap();
    assert_eq!(hits_mod.len(), 1);
    assert_eq!(hits_mod[0].title, "Page Beta Modified");

    // 4. Delete C.md
    fs::remove_file(wiki_root.join("C.md")).unwrap();
    let existing2 = terminalwiki_index::store::load_index(&idx_dir).unwrap();
    let updated_entries2 =
        terminalwiki_index::indexer::update_index(&wiki, &config, existing2).unwrap();
    terminalwiki_index::store::save_index(&idx_dir, &updated_entries2).unwrap();

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
    let existing3 = terminalwiki_index::store::load_index(&idx_dir).unwrap();
    let updated_entries3 =
        terminalwiki_index::indexer::update_index(&wiki, &config, existing3).unwrap();
    terminalwiki_index::store::save_index(&idx_dir, &updated_entries3).unwrap();

    let store3 = TantivyStore::open_reader(&idx_dir).unwrap();
    let q_ren = Query::from_str("allocation").unwrap();
    let hits_ren = store3.search(&q_ren, 10).unwrap();
    assert_eq!(hits_ren.len(), 1);
    assert_eq!(hits_ren[0].relative, Path::new("X.md"));
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
    let entries = terminalwiki_index::indexer::update_index(&wiki, &config, None).unwrap();
    terminalwiki_index::store::save_index(&idx_dir, &entries).unwrap();

    let meta_before = fs::read_to_string(idx_dir.join("state.json")).unwrap();

    // Execute search 5 times
    for _ in 0..5 {
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

    // 1. Candidate fallback: index.md
    fs::write(wiki_root.join("index.md"), "# Index").unwrap();
    let wiki = Wiki::open(&entry).unwrap();
    assert_eq!(wiki.home_page(), Some(wiki_root.join("index.md")));

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
        Some(wiki_root.join("CustomHome.md"))
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
                                   // Check line 2 (let x = 1) has tab expanded to 4 spaces
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
    assert_eq!(display_width(emoji), 23);
    assert_eq!(display_width(japanese), 16);
    assert_eq!(display_width(german), 18);

    let truncated = truncate_display_width(japanese, 8);
    assert_eq!(display_width(&truncated), 8);

    let padded = pad_display_width("Heap", 10);
    assert_eq!(display_width(&padded), 10);
}
