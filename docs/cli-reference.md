# CLI Reference

TerminalWiki provides a concise Unix-style command surface.

## Syntax

```text
tw [OPTIONS] [WIKI] [PAGE]
tw <COMMAND> [ARGS...]
```

## Primary Commands

### `tw [PAGE]`
Resolves and displays a page or wiki overview.

### `tw search <QUERY>`
Full-text search using Tantivy:
- Filter by tag: `tw search "tag:security heap"`
- Filter by type: `tw search "type:code malloc"`
- Filter by path: `tw search "path:kernel scheduler"`
- Negative filter: `tw search "tag:security -tag:web heap"`

### `tw find <QUERY>`
Instant fuzzy search matching page titles, filenames, and aliases with Nucleo.

### `tw tui [PAGE]`
Launches the full interactive terminal interface.

### `tw new <PAGE>`
Creates a new markdown file with frontmatter and opens it in your editor.

### `tw edit <PAGE>`
Opens an existing page in `$EDITOR` (`nvim`, `vim`, `nano`).

### `tw backlinks <PAGE>`
Lists all notes linking to the target page.

### `tw links <PAGE>`
Lists all outgoing links from the target page.

### `tw related <PAGE>`
Lists deterministically ranked related pages based on graph connectivity.

### `tw graph [PAGE]`
Renders ASCII link graph or exports Graphviz DOT format with `--format dot`.

### `tw tags` / `tw tag <TAG>`
Lists all tags with page counts or lists pages with a specific tag.

### `tw wiki <list|add|remove|rename|default>`
Manages multi-wiki configurations.

### `tw index <status|update|rebuild>`
Inspects and maintains the Tantivy index.

### `tw lint`
Validates wiki integrity: detects broken `[[WikiLinks]]`, missing images, and invalid frontmatter.

### `tw doctor`
Prints system diagnostics, configuration status, and detected terminal capabilities.

### `tw config`
Displays active merged configuration.
