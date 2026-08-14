# Indexing & Search Architecture

## Persistent Full-Text vs. In-Memory Fuzzy Search

TerminalWiki divides search into two specialized, high-performance tiers:

```text
               ┌───────────────────────┐
               │    Files on Disk      │
               └───────────┬───────────┘
                           │ Scan & Hash (BLAKE3)
          ┌────────────────┴────────────────┐
          ▼                                 ▼
┌───────────────────┐             ┌───────────────────┐
│   Tantivy Index   │             │   Nucleo Finder   │
│ (Persistent Disk) │             │    (In-Memory)    │
│  - Full text      │             │  - Page titles    │
│  - Body content   │             │  - Relative paths │
│  - BM25 ranking   │             │  - Aliases / tags │
│  - Snippet context│             │  - Instant typing │
└───────────────────┘             └───────────────────┘
```

1. **Tantivy (`tw search`)**: Persistent on-disk inverted index per wiki located at `~/.cache/terminalwiki/index/<wiki>/`. Handles BM25 ranked full-text queries, structured AST filters (`tag:`, `wiki:`, `type:`, `ext:`, `path:`, `title:`, `-not`), and contextual snippets.
2. **Nucleo (`tw find` & TUI Picker)**: In-memory fuzzy matcher for instant sub-millisecond page and title navigation.

## Schema Versioning & Self-Healing

The index schema version is tracked in `meta.json` (`schema_version = 2`). If a schema mismatch or corruption is detected, the index is automatically rebuilt from the original files on disk.

## Incremental Updates

During `tw index update`:
1. Each file's `mtime` and `size` are checked.
2. If unchanged, the existing entry is preserved.
3. If changed, the file is re-parsed and its Tantivy document updated.
4. Deleted files are removed from both `entries.jsonl` and Tantivy.
