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
| `cargo test --workspace --all-features` | 0 | PASS — **184 passed, 0 failed** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 | PASS — 0 warnings |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps` | 0 | PASS — 0 warnings |
| `cargo build --release --workspace` | 0 | PASS — release binary produced |
| `sphinx-build -W --keep-going -b html docs docs/_build/html` | — | **NOT RUN** — `sphinx-build` is not installed on this machine |
| Read the Docs build | — | **NOT RUN** — never executed against the RTD service |

### Test counts by target

| Target | Passed |
|---|---|
| `terminalwiki-core` (lib) | 152 |
| `terminalwiki-render` (lib) | 14 |
| `terminalwiki-index` (lib) | 7 |
| `terminalwiki-graph` (`tests/graph_tests.rs`) | 6 |
| `terminalwiki` (`tests/integration_tests.rs`) | 5 |
| **Total** | **184** |

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
| 6 | Graph CLI scaling (thresholds, `--max-nodes`, TTY size, Unicode width) | Not started |
| 7 | **Graph TUI** (`Mode::Graph`, background layout, generation IDs) | **Not started** — no `Mode::Graph` exists |
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
