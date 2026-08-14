# Search and Incremental Indexing

## Tantivy Search Engine

TerminalWiki uses an embedded **Tantivy** engine for BM25-ranked full-text search.

### Schema Fields
- `title`: Boosted exact match (10x) and word match (5x)
- `aliases`: Frontmatter aliases (3x boost)
- `headings`: Markdown headings (2x boost)
- `body`: Sanitized plain body text (1x boost)
- `tags`: Exact keyword match
- `content_type`: `markdown`, `code`, `latex`, `image`, `binary`
- `extension`: e.g. `md`, `rs`, `c`, `tex`

## Incremental Delta Model

When running `tw index update`:
1. Scans filesystem metadata (`mtime` + `size`). Unmodified files are instantly skipped.
2. If `mtime` or `size` differ, a BLAKE3 content hash is computed. If the hash matches previous state, the file was only touched without text changes and is skipped.
3. Only new or truly modified files are re-parsed.
4. Tantivy atomic delta mutations delete old terms by stable `doc_id` and insert updated documents in a single atomic commit.

## Cache Independence

The index resides in `~/.cache/terminalwiki/index/<wiki>/`. Deleting this directory causes zero data loss: running `tw index rebuild` reconstructs the entire index from source markdown files in seconds.
