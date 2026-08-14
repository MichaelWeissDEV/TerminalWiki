# Current Development Status

Date: 2026-08-14
Rust: 1.82+ (2021 edition)
Milestone: **Alpha Phase 1 (Consolidation & Robustness)**

## Checks

- `cargo check --workspace --all-targets --all-features`: PASS (0 errors, 0 warnings)
- `cargo build --workspace --release`: PASS
- `cargo clippy`: Clean
- Unit & Architecture Tests: Verified

## Completed In This Phase

1. **Default Wiki & Home Resolution**:
   - Centralized `WikiSet::default_wiki()` and `WikiSet::get()`.
   - Strict fallback priority: `.terminalwiki.toml` (`home = "..."`) -> `index.md` -> `README.md` -> `Home.md` -> `home.md` -> automatic generated overview.
2. **Content Classification**:
   - Centralized `ContentType` classification (`classify_file(&Path)` / `classify(&Path, &[u8])`).
   - Source code files bypass markdown parser and route directly to the code renderer.
3. **Syntax Highlighting & Code Renderer**:
   - Integrated `syntect` using base16 palettes.
   - Complete preservation of indentation, whitespace, and column structure.
   - Line numbers and range highlighting support.
4. **Markdown AST & Semantic Links**:
   - `pulldown-cmark` parsing into structured `Document` AST (`Block` & `Inline`).
   - Tables with alignment and single-line rules `───`.
   - Callouts (`[!NOTE]`, `[!TIP]`, `[!WARNING]`, `[!CAUTION]`, `[!IMPORTANT]`).
   - Embedded link tracking for interactive TUI cycling (`Tab`/`Enter`).
5. **Search Engine Upgrade**:
   - Persistent `Tantivy` full-text search with schema versioning (`INDEX_SCHEMA_VERSION = 2`), BM25 ranking, and contextual snippet generation.
   - In-memory `Nucleo` fuzzy finder for instant sub-millisecond page finding.
   - Full structured query AST parsing (`tag:`, `wiki:`, `type:`, `ext:`, `path:`, `title:`, `-not`).
6. **Machine-Readable Outputs**:
   - Strict `serde_json` serialization for `--json` and `--jsonl`.
   - Clean stdout (data) vs. stderr (diagnostics/progress) separation.
7. **TUI Overhaul**:
   - Clean, minimalist viewport layout without superfluous boxes.
   - Integrated Nucleo fuzzy finder (`f` / `Ctrl+p`).
   - Document outline section jumper (`o`).
   - Backlinks pane (`b`) and safe `$EDITOR` suspension (`e`).
8. **Benchmarks & Fixture Generator**:
   - Implemented `tw-bench-gen` binary for generating synthetic 1k, 10k, 100k page wikis.
   - Benchmark suite measuring Markdown parsing, syntect highlighting, and Nucleo fuzzy search.
9. **Documentation**:
   - Created `docs/architecture/` (`filesystem-model.md`, `indexing.md`, `rendering.md`, `security.md`).
   - Created `README.md`.

## Next Phase Goals (Deferred)
- Rich media / Kitty / Sixel image protocols
- Full LaTeX pixel rendering
- File watcher daemon / background auto-indexing
