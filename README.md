# TerminalWiki

A fast, terminal-native knowledge base. **The files are the truth.**

TerminalWiki (`tw`) provides an instant view, index, and navigation layer over plain Markdown and source code directories. No proprietary database is required; if all TerminalWiki cache and indices are deleted, zero knowledge is lost.

---

## Status

TerminalWiki is in **Alpha** (Phase 1 consolidation complete). Core addressing, Tantivy full-text indexing, Nucleo fuzzy finding, syntect source code rendering, and interactive TUI navigation are fully operational.

---

## Installation

### Prerequisites
- Rust 1.82 or newer
- Cargo

### Building from Source
```bash
git clone https://github.com/MichaelWeissDEV/TerminalWiki.git
cd terminalwiki
cargo build --release
```

The binaries `tw` and `terminalwiki` will be placed in `target/release/`.

---

## Quick Start

### 1. Register a Wiki
```bash
# Add an existing folder as the default knowledge base
tw wiki add main ~/Knowledge --default
```

### 2. Open Pages
```bash
tw                          # Opens default wiki home page
tw "Heap Exploitation"      # Resolves page by title, stem, or alias
tw src/main.rs              # Views source code with syntax highlighting & line numbers
```

### 3. Full-Text Search (Tantivy)
```bash
tw search allocator                     # Full-text search with contextual snippets
tw search tag:security -tag:web heap    # Structured search query
tw search malloc --json | jq .          # Machine-readable JSON output
```

### 4. Fuzzy Find (Nucleo)
```bash
tw find heap                # Instant fuzzy page search
```

### 5. Interactive TUI
```bash
tw tui                      # Opens terminal interactive interface
```

---

## Commands Overview

| Command | Description |
|---|---|
| `tw [PAGE]` | Resolve and display a page or source file |
| `tw search <QUERY>` | Tantivy full-text search with snippet context and filters |
| `tw find <QUERY>` | Nucleo-powered fuzzy page and title search |
| `tw tui` | Interactive TUI with instant fuzzy finder, outline, and links |
| `tw new <PAGE>` | Create a new page with frontmatter template and launch `$EDITOR` |
| `tw edit <PAGE>` | Open an existing page in `$EDITOR` |
| `tw delete <PAGE>` | Delete a page with confirmation |
| `tw backlinks <PAGE>` | Show incoming links to a page |
| `tw links <PAGE>` | Show outgoing links from a page |
| `tw related <PAGE>` | Show deterministically ranked related pages |
| `tw graph [PAGE]` | Render ASCII/Unicode link graph or Graphviz DOT |
| `tw tags` | List all tags and page counts |
| `tw tag <TAG>` | Filter pages by tag |
| `tw wiki ...` | Manage registered wikis (list, add, remove, mount, default) |
| `tw index ...` | Manage search index (status, update, rebuild) |
| `tw lint` | Check wiki for broken links and invalid frontmatter |
| `tw doctor` | System diagnostic report |
| `tw config` | Show active configuration |

---

## TUI Keybindings

- `j` / `k` / `↓` / `↑`: Scroll down / up
- `Ctrl+d` / `Ctrl+u`: Half-page down / up
- `g` / `G`: Jump to top / bottom
- `f` / `Ctrl+p`: Instant fuzzy page finder
- `o`: Document outline jump
- `b`: Backlinks pane
- `Tab` / `Shift-Tab`: Cycle through document links
- `Enter`: Open selected link / item
- `Ctrl+o` / `Ctrl+i`: Backward / forward navigation history
- `e`: Suspend TUI and edit in `$EDITOR`
- `?`: Toggle keybinding help
- `q` / `Esc`: Quit / Close modal

---

## Supported File Types

- **Markdown (`.md`, `.markdown`)**: Headings, lists, tables, callouts (`[!NOTE]`, etc.), semantic wiki links (`[[...]]`), footnotes.
- **Source Code (`.rs`, `.c`, `.cpp`, `.py`, `.go`, `.js`, `.ts`, `.sh`, etc.)**: Highlighted via syntect with exact whitespace preservation and line numbering.
- **Plain Text / LaTeX / Images / Binaries**: Sniffed and handled with appropriate metadata or text viewers.

---

## Security Model

- **Root Confinement**: All file resolution is contained within the wiki root; path traversal attempts (`../../`) and escaping symlinks are rejected.
- **Terminal Sanitization**: All content, search results, and metadata are sanitized against harmful ANSI/OSC escape injection before terminal output.
- **Clean Editor Suspension**: Terminal raw mode is cleanly dropped and restored when launching external editors.

---

## Development

```bash
# Check compilation
cargo check --workspace

# Run tests
cargo test --workspace

# Linting
cargo clippy --workspace --all-targets --all-features

# Formatting
cargo fmt --all

# Benchmarks
cargo run --bin tw-bench-gen -- 10000 target/bench_10k
cargo run --bench benchmark_suite
```

---

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE).
