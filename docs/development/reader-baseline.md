# Reader beta baseline

**Recorded:** 2026-11-02  
**Checkout:** `19ed03c` on local branch `master`

## Working-tree protection

Before this pass, `crates/terminalwiki-render/src/{ansi,binary,code_view,document,highlight,lib,markdown,render,theme}.rs`
already had uncommitted modifications. They are treated as user work and were
not overwritten or reset. `MASTER_PROMPT.md` and `IMPLEMENTATION_SPEC.md` are
also untracked local inputs.

## Tooling result

| Command | Exit/result | Notes |
| --- | --- | --- |
| `cargo --version` | `0` | Cargo 1.99.1 installed via `rustup` for the current user. |
| `cargo fmt --all -- --check` | `0` | PASS |
| `cargo check --workspace --all-targets --all-features` | `0` | PASS |
| `cargo test --workspace --all-features` | `0` | PASS — 229 unit/integration tests plus doc-test targets completed. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | `0` | PASS |
| `sphinx-build -n -W -b html docs docs/_build/html` | not run | `sphinx-build` is not installed. |

`git diff --check` also exits successfully. A local smoke run in a temporary
wiki verified `tw init --name knowledge` followed by `tw --plain`; it created
and displayed `index.md` without touching a user wiki.

## Phase-0 security change

The former `tw new` path was constructed with `wiki.root.join(filename)`. The
creation path now goes through `Wiki::resolve_within`, which canonicalizes the
nearest existing ancestor; therefore a non-existent target under an in-wiki
symlink to an outside directory is refused. Creation uses `OpenOptions::create_new`
instead of a separate existence check. Regression tests cover traversal/symlink
refusal in both core containment and `tw new` target selection (Unix).

External editor execution remains direct `Command` argv invocation. Its command
string is now quote-aware tokenized centrally in `terminalwiki-core` and
malformed quoting is rejected; the CLI and TUI both use it and no shell is
invoked.

`tw init [NAME]` / `tw init --name NAME` is also present as a minimal bootstrap:
it creates only missing `index.md` and `.terminalwiki.toml`, then registers the
current directory as the default wiki. It deliberately does not overwrite an
existing document or local config.

## Render recovery and native math

The pre-existing render diff removed the public render crate API and made the
TUI fail to compile. It was saved before recovery at
`/tmp/opencode/terminalwiki-render-preexisting.patch`; the tracked renderer was
then restored to its local `HEAD` basis so the implementation can be integrated
and tested. The native renderer now converts a bounded TeX subset to Unicode
layout for fractions, roots, Greek letters, relations, sums/integrals, scripts,
and basic matrices/cases. Unsupported commands remain visible and produce a
warning. This is deliberately not a complete TeX implementation.

## Remaining verification

Run a real PTY/TUI and graphics-terminal matrix, plus the strict Sphinx build
after installing `sphinx-build`. PDF/image raster backends, standalone LaTeX
documents, diagrams, and packaging remain incomplete beta work.
