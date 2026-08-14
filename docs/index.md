# TerminalWiki Documentation

Welcome to the documentation for **TerminalWiki** (`tw`), a fast, terminal-native knowledge base and markdown viewer implemented in Rust.

```{toctree}
:maxdepth: 2
:caption: User Guide

getting-started
cli-reference
tui-guide
search-and-index
graph-and-links
configuration
```

```{toctree}
:maxdepth: 2
:caption: Technical Reference

architecture
development/current-status
```

## Core Philosophy

> **The files are the truth.**
>
> TerminalWiki is a fast view, search index, and navigation layer over plain files on disk. No proprietary database is required to read or edit your notes.

## Key Features

- ⚡ **Instant Full-Text Search**: Powered by Tantivy with exact title boosting, field filters, and incremental updates.
- 🔍 **Fuzzy Finder**: High-performance in-memory fuzzy finder using Nucleo.
- 🎨 **Terminal-Native Rendering**: Markdown and code highlighting with Syntect, tabstop support, and image metadata cards.
- 🔗 **Bi-directional Link Graph**: Interactive terminal graph, backlinks, broken link detection, and DOT export.
- 🖥️ **Interactive TUI**: Vim-like navigation, inline fuzzy finder, document outline, backlinks viewer, and command palette.
- 🔄 **Incremental Delta Indexing**: BLAKE3 content-hash verification skipping unmodified files.
