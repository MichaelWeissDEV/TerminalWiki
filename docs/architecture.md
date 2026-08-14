# Architecture

TerminalWiki is organized as a Cargo workspace with clean separation of concerns across focused crates:

```text
terminalwiki/
├── crates/
│   ├── terminalwiki-core    # Domain models, paths, sanitization, config, file watching
│   ├── terminalwiki-render  # Pulldown-cmark parser, Syntect highlighter, terminal cards
│   ├── terminalwiki-index   # Tantivy engine, Nucleo fuzzy finder, Delta indexer
│   ├── terminalwiki-graph   # Adjacency graph, related scoring, DOT export, layout
│   ├── terminalwiki-tui     # Interactive Crossterm TUI, outline, backlinks, command mode
│   └── terminalwiki-cli     # CLI entry point, hand-written argument parser, commands
└── src/                     # Main binary launcher (tw / terminalwiki)
```

## Security and Terminal Sanitization

Terminal output from untrusted files is strictly sanitized:
- Raw terminal escape sequences (`\x1b`, CSI, OSC controls) in document content are filtered before writing to stdout.
- Binary files are sniffed with magic numbers and never decoded into raw UTF-8 streams.
- Path traversal outside configured wiki roots is strictly prevented.
