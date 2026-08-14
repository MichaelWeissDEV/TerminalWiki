# Getting Started

## Installation

### From Source

Ensure you have Rust 1.82+ installed:

```bash
git clone https://github.com/MichaelWeissDEV/TerminalWiki.git
cd TerminalWiki
cargo build --release
```

The resulting binary will be located at `target/release/tw`. Both `tw` and `terminalwiki` are supported.

## Quick Start

### 1. Register a Wiki

Add an existing directory containing markdown notes:

```bash
tw wiki add notes ~/Knowledge --default
```

### 2. Build the Search Index

Generate the persistent Tantivy search index and metadata cache:

```bash
tw index rebuild
```

### 3. Browse and Search

Display the home overview:
```bash
tw
```

Search your knowledge base:
```bash
tw search "memory allocation"
```

Fuzzy find a note:
```bash
tw find heap
```

Launch the interactive TUI:
```bash
tw tui
```
