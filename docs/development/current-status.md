# TerminalWiki — Current Development & Architecture Status

**Status:** Alpha
**Target:** 0.1.0-rc.1
**Cargo version:** `0.1.0-alpha`
**Date:** 2026-08-15
**Repository:** [https://github.com/MichaelWeissDEV/TerminalWiki](https://github.com/MichaelWeissDEV/TerminalWiki)

> This project is **not** a release candidate. `0.1.0-rc.1` may only be tagged
> once every item in "Remaining Work Before RC" below is closed and verified.

---

## Gate 0 — Verified Build & Test Truth

All rows below were produced by actually running the command on the commit named
in "Baseline". No row is asserted from expectation.

**Baseline commit:** `45a585a` (working tree contains the changes described under
"Changes Landed In This Pass")
**Toolchain:** `rustc 1.97.1 (8bab26f4f 2026-07-14) (Homebrew)`,
`cargo 1.97.1 (c980f4866 2026-06-30) (Homebrew)`
**Platform:** macOS (darwin 25.4.0), arm64

| Command | Exit | Result |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | PASS — no diffs |
| `cargo check --workspace --all-targets --all-features` | 0 | PASS — 0 errors, 0 warnings |
| `cargo test --workspace --all-features` | 0 | PASS — **196 passed, 0 failed** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | PASS — 0 warnings |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps` | 0 | PASS — 0 warnings |
| `cargo build --release --workspace` | 0 | PASS — release binary produced |
| `sphinx-build -W --keep-going -b html docs docs/_build/html` | — | **NOT RUN** — `sphinx-build` is not installed on this machine |
| Read the Docs build | — | **NOT RUN** — never executed against the RTD service |

### Test counts by target

| Target | Passed |
|---|---|
| `terminalwiki-core` (lib) | 152 |
| `terminalwiki-render` (lib) | 16 |
| `terminalwiki-index` (lib) | 13 |
| `terminalwiki-graph` (`tests/graph_tests.rs`) | 8 |
| `terminalwiki` (`tests/integration_tests.rs`) | 5 |
| `terminalwiki-tui` (`tests/graph_view_tests.rs`) | 5 |
| **Total** | **196** |

Binary and doc-test targets report 0 tests and are omitted.

---

## Starting State (before this pass)

The suite was **red**: 5 failures across 3 targets. Recorded here because the
previous revision of this file claimed "Release Candidate 1.0 (Full Completion
Plan Realized)" with every gate marked PASS, while `cargo test` was not part of
that checklist and did not pass.

| Failing test | Verdict |
|---|---|
| `render::markdown::test_wiki_links_and_aliases` | **Product bug** — fixed in code |
| `integration::test_index_lifecycle_flow` | Test-side — fixture arithmetic error |
| `integration::test_home_page_resolution_priority` | Test-side — macOS symlink |
| `integration::test_unicode_width_and_rendering` | Test-side — two wrong expectations |
| `core::unicode::truncates_without_breaking_graphemes` | Test-side — over-truncating expectation |

---

## Changes Landed In This Pass

### Gate 2.1 / 2.2 — Indexer I/O errors are no longer swallowed

`compute_delta` previously read file contents with
`fs::read(&path).unwrap_or_default()`. An unreadable file therefore hashed as
**0 bytes**, which silently replaced a perfectly good index entry with empty
content.

Replaced with explicit classification:

- **`NotFound`** — the scan and the read are not atomic, so another process may
  delete a file in between. The path is dropped from the current delta and from
  `seen_paths`, so the next pass observes it as an ordinary deletion. It is
  reported as `SkipReason::Vanished`.
- **Any other I/O error** (permission denied, broken symlink, hardware error) —
  the **previously indexed entry is retained** rather than destroyed, and the
  file is reported as `SkipReason::Unreadable`.

Both are surfaced through a new `IndexDelta::skipped: Vec<SkippedFile>` field,
carried out through `WikiIndex::update`, and **printed to stderr by
`tw index update`** — reporting into a struct nobody reads would not satisfy
"melden". Paths and error text pass through `sanitize_line` before display.
The same `NotFound` tolerance was applied to `build_all`, where a mid-rebuild
deletion previously aborted the whole rebuild.

Verified against the release binary:

```
$ tw index update
Updating index for 'smoke' … done (2 pages, 0.0s)
  warning: could not read Allocator.md: Permission denied (os error 13) (keeping previously indexed content)
```

A subsequent `tw search` still returned the previously indexed content, confirming
the entry was retained rather than blanked.

Regression tests: `unreadable_file_is_reported_and_retains_previous_content`
(chmod 000, asserts the original content hash survives) and
`vanished_file_falls_out_of_delta`. A guard test asserts the suite is not running
as root, which would make the permission test vacuous.

### Gate 2.3 — Tantivy `expect()` removed

`TantivyStore::open_or_create` called
`Index::create_in_dir(...).expect("create tantivy index")`, aborting the process
on disk-full, permission, or corruption errors. It now returns
`Result`, reporting both the open failure and the subsequent create failure.

A census of the whole `terminalwiki-index` crate's production code (excluding
`#[cfg(test)]` modules) for `unwrap()` / `expect()` / `panic!` / `todo!` /
`unimplemented!` found exactly one other hit: `search.rs` sorted results with
`partial_cmp(...).unwrap()`, which panics on a NaN score inside a user-facing
search path. Changed to `total_cmp`. **The index crate's production code now
contains none of those constructs.** The equivalent census for the other crates
has not yet been done — that is Gate 28 and remains open.

### Gate 2.4 — Search read-only

A read-only test already existed, but it ran only **5** searches and compared
only `state.json` — it could not have detected a rebuilt Tantivy segment.
Strengthened to the spec's actual requirement: **100** searches, plus a snapshot
of every file under the index directory (relative path, length, mtime) taken
before and after and compared. Segment creation, deletion, or rewriting now fails
the test.

### Wiki links — real product bug

`parse_markdown` never set `Options::ENABLE_WIKILINKS`. CommonMark therefore
parsed `[[Heap]]` as a nested link reference and split it across several
`Event::Text` chunks, so the hand-written `parse_wiki_links_in_text` scanner
could never observe a complete `[[...]]` span — it emitted `Text("[")` instead.

Confirmed by dumping the raw pulldown-cmark event stream. Fixed by enabling the
option and handling `LinkType::WikiLink { has_pothole }` natively, so
`[[Target]]` and `[[Target|Label]]` both produce `Inline::WikiLink`.

### Frontmatter leaked into every rendered page — real product bug

Found by smoke-testing the release binary, not by the test suite. `tw Heap`
printed:

```
## title: Heap tags: [security]
```

`render_markdown` passed raw file text straight to the parser, so the `---`
fences were parsed as a **setext heading** and the YAML keys were displayed as
page content. The indexer stripped frontmatter via `Frontmatter::body_offset`;
the renderer did not. This affected the CLI and the TUI alike, since both reach
markdown through this one entry point.

Fixed by stripping at `render_markdown` using the same `body_offset` the indexer
uses, so display and index agree on where the body begins.

The strip is deliberately **not** applied to `ContentType::Text`. The indexer
does not parse frontmatter for plain text, so stripping there would make display
and index disagree and would silently hide everything between a leading `---` and
the next one in a `.txt` file. `render_path` therefore routes Markdown through
`render_markdown` and Text through the new `render_markdown_raw`, which
`tw render` also uses.

Tests cover: the stripped case, the no-frontmatter case, a mid-document `---`
rule (content, must survive), and the same bytes rendered as `.md` versus `.txt`.

### Wiki links were never indexed — the bug behind broken backlinks and graph

Found while writing the graph-view tests: the neighbourhood of a page that
plainly links to three others contained **one** node.

`indexer::parse_markdown` called `find_links` **per `Event::Text`**. `find_links`
is a raw-text scanner that needs a complete `[[...]]` span, but CommonMark splits
that span across several text events — so it matched nothing and `wiki_links` was
always empty. Verified against the real index before the fix:

```
Allocator.md -> []
Heap.md      -> []          # this file contains [[Allocator]] twice
```

Because every consumer reads `wiki_links`, this silently broke **backlinks,
outgoing links, related pages, and the entire graph** — all four are Definition
of Done items that appeared to work only because they degrade to "nothing found"
rather than failing loudly.

Fixed by scanning the whole document once and storing the resolved page name
(`Heap`), which is what the graph resolves against, rather than the raw bracket
text. Verified end to end:

```
$ tw links Heap        →  Outgoing links from 'Heap.md' (2)
$ tw backlinks Allocator →  Pages linking to 'Allocator.md'
$ tw related Heap      →  smoke:Allocator.md · score 20.0 (Direct link)
```

Backlinks are now also deduplicated per source page: a page that links to the
target three times is one backlink, not three, which otherwise inflated the
count shown in the article header.

### Interactive graph view (priority 1 of the supplementary spec)

`Mode::Graph` added to the TUI, using `terminalwiki-graph` — no second graph
implementation. `g` and `:graph` both open the local graph of the current page at
depth 2; `j`/`k`/`Tab` select, `Enter` opens the node (pushing history exactly
like following a link), `+`/`-` change depth, `r` rebuilds, `Esc` returns, `?`
lists the keys.

`:graph` was previously in the command list and autocompleted, but had no match
arm — it reported "Unknown command: graph". It is now wired to the same entry
point as `g`.

Supporting changes:

- **The graph is cached on `App`.** `load_backlinks` rebuilt the entire
  `WikiGraph` — walking every entry of every wiki — on *every* `b` press. It is
  now built once, lazily, and shared with the graph view.
- **`neighborhood_limited`** caps nodes during a breadth-first traversal, so the
  cap keeps the centre's nearest neighbours and bounds the O(n²) layout input
  rather than discarding work after the fact. Edges whose endpoints do not
  survive the cap are dropped.
- **`render_graph` now returns `GraphRender`** — the canvas plus a
  `LabelPlacement` per node. Selection cannot be implemented by searching the
  rendered rows for the title, because duplicate and truncated labels make that
  ambiguous.
- **Unicode-correct canvas.** Labels were measured with `char` count, so a CJK or
  emoji title shifted its whole row and skewed every edge through it. Labels are
  now measured with `display_width`, drawn by grapheme, and wide graphemes
  reserve a continuation cell. A test asserts every canvas row is exactly the
  requested display width for `Heap`, `Größe`, `日本語`, and `🦀 Rust`.
- Labels may overwrite edge glyphs but never another label, so nodes no longer
  render as an unlabelled bare marker when a line passes behind them.

`g` previously meant "scroll to top"; per supplementary spec item 101 it now
opens the graph, and `Home` remains scroll-to-top. The help screen reflects this.

Tested by `crates/terminalwiki-tui/tests/graph_view_tests.rs` (5 tests: open and
centre selection, selection clamping, Enter-opens-node plus history, depth change
and clamping, `:graph` wiring, cache reuse and invalidation). These build a real
wiki and index with `TW_CACHE_DIR` pointed at a temp directory, so they never
touch the developer's cache.

### Gate 1 — Version coherence

`crates/terminalwiki-cli/src/lib.rs` printed a **hardcoded** `"terminalwiki 0.1.0"`
in both `--version` and `print_help()`, which would silently drift from Cargo
metadata. Both now read `env!("CARGO_PKG_VERSION")`.

Workspace version set to `0.1.0-alpha`, including the `version =` requirements in
`[workspace.dependencies]` — a prerelease does not satisfy a plain `^0.1.0`
requirement, so those had to move together. `docs/conf.py` (`release`) and
`man/tw.1` updated to match. README already stated Alpha.

---

## Remaining Work Before RC

These are **not started or stub-only**. This list is the honest gap between the
current tree and the RC Definition of Done.

| Gate | Feature | State |
|---|---|---|
| 3 | Search query semantics (`title_exact`, phrase, path, `type:`, negation) | Not started |
| 4 | Structured search snippets (currently HTML-ish string handling) | Not started |
| 5 | Fuzzy search consolidation on Nucleo | Partially present |
| 6 | Graph CLI scaling | **Partly done** — node cap and Unicode-correct label widths landed with the graph view. Still open: `tw graph` with no page still materialises the whole graph, `--max-nodes` is not a CLI flag, and width still comes from `COLUMNS` rather than the real TTY size. |
| 7 | Graph TUI — view, selection, navigation, depth | **Done** (see above) |
| 7.11-7.13 | Graph background layout, generation IDs, cancellation | **Deferred, deliberately** — the layout is computed once per open (not per frame) and is milliseconds once the graph is cached, so the cost was never the layout. The event-loop restructuring a worker needs is the same one the file watcher requires; doing it once there avoids touching graph code twice. |
| 8 | **File watcher integration into the TUI** | **Not started** — `watch.rs` exists in core but the TUI never constructs a watcher |
| 9 | Terminal capability layer (pixel size, mouse, tmux, SSH) | `caps.rs` has the protocol enum only |
| 10 | **Native image rendering** (kitty / iTerm2 / sixel / unicode backends) | **Not started** — only a `GraphicsProtocol` enum and config plumbing; no backend renders anything |
| 11 | **Math rendering** | **Stub** — `math.rs` is 16 lines emitting `⟨formula⟩`; `render.rs` hardcodes that form rather than delegating |
| 12 | Post-rich-media terminal security audit | Blocked on 10/11 |
| 13 | TUI responsive design finalization | Partially present |
| 14 | Full Read the Docs structure (~60 pages) | 15 files present vs. target tree |
| 16–19 | Regression suite, fuzzing, performance, memory | Not started |
| 22–27 | Release smoke test, pipes, terminal matrix, packaging, completions | Not started |

### Known limitations

- **Sphinx is not installed** on the development machine, so the strict docs
  build (Gate 21) has never been executed here. Any future "Docs: PASS" claim
  must cite a real run.
- The permission-denied indexer test is `#[cfg(unix)]` and is skipped on Windows.
- Windows is **untested**, not supported.
