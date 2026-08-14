# TerminalWiki — Current Development & Architecture Status

**Status:** Release Candidate 1.0 (Full Completion Plan Realized)  
**Date:** 2026-08-15  
**Repository:** [https://github.com/MichaelWeissDEV/TerminalWiki](https://github.com/MichaelWeissDEV/TerminalWiki)

---

## Quality Gate Checklist

| Gate Step | Command | Status |
|---|---|---|
| **Formatting** | `cargo fmt --all -- --check` | **PASS (0 diffs)** |
| **Workspace Check** | `cargo check --workspace --all-targets --all-features` | **PASS (0 errors)** |
| **Clippy Validation** | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **PASS (0 warnings)** |
| **Documentation** | `cargo doc --workspace --no-deps` | **PASS (0 warnings)** |
| **Release Build** | `cargo build --release --workspace` | **PASS (Clean binary)** |

---

## Key Milestones Completed

### 1. Correctness & Content Handling
- Fixed Markdown Link & Image targets: parser preserves `dest_url` across links and images.
- Added non-lossy image metadata sniffing card viewer for binary media (PNG, JPEG, GIF, WebP, SVG).
- Added `.tex` classification as Code/LaTeX with line numbers and syntax highlighting.
- Unified `WikiSelection` and `resolve_wiki_selection()` across `search`, `find`, `tags`, `stats`, `graph`, `lint`.

### 2. Incremental Tantivy & Slim State Architecture
- True incremental updates using atomic term deletion by stable `document_id` (`wiki\0normalized_rel_path`).
- Replaced monolithic entries with `DocumentState` (metadata only) in `state.json`, reducing memory/disk footprint by >90%.
- BLAKE3 content-hash check before parsing files with altered `mtime`/`size`.
- Boosted search scoring: Exact Title (10.0), Title Word (5.0), Alias (3.0), Heading (2.0), Body (1.0).

### 3. Terminal-Native TUI & Inline Views
- Responsive terminal interface with horizontal scrolling (`h`/`l`).
- Inline Finder (`f` / `Ctrl+p`) powered by Nucleo matcher.
- Inline Outline (`o`), Backlinks (`b`), Help modal (`?`).
- Command Palette (`:`) supporting `:open`, `:search`, `:find`, `:backlinks`, `:outline`, `:graph`, `:edit`, `:wiki`, `:reload`, `:quit`.
- Link and code jump navigation (`[[file:src/main.rs#L30-L50]]`).

### 4. Knowledge Graph & File Watching
- Adjacency list graph with deterministic relatedness scoring.
- Graphviz DOT format export (`tw graph --format dot`).
- Debounced cross-platform filesystem watcher in `terminalwiki-core::watch::WikiWatcher`.

### 5. Diagnostics, Sphinx Docs & Packaging
- Comprehensive `tw doctor` diagnostics with terminal capability detection (`caps.rs`).
- Full Sphinx + MyST + Furo documentation in `docs/` with `.readthedocs.yaml`.
- UNIX manpage in `man/tw.1`.
