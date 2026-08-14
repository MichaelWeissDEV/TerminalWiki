# Filesystem Model

## Core Principle: The Files are the Truth

TerminalWiki is designed around the invariant that **markdown and code files on disk are the sole source of truth**. TerminalWiki never copies, moves, or rewrites content into a proprietary database. If all TerminalWiki caches and indices are removed at any time, zero data is lost, and the search index can be completely reconstructed via `tw index rebuild`.

## Wiki Hierarchy & Addressing

A wiki is a standard filesystem directory containing Markdown, code, and asset files.

1. **Host Wikis**: Configured in `~/.config/terminalwiki/config.toml` (or local `.terminalwiki.toml`).
2. **Subwiki Mounts**: Logical mounts linking child wikis into parent wikis (`tw wiki mount parent child`).
3. **Addressing Syntax**:
   - `tw` → Home page of default wiki
   - `tw PAGE` → Resolves page across default wiki and mounts
   - `tw WIKI` → Resolves home page of named wiki
   - `tw @WIKI PAGE` → Resolves page strictly within named wiki
   - `tw page NAME` → Forces resolution as a page rather than wiki name

## Home Page Resolution Sequence

When addressing a wiki without a specific page, TerminalWiki resolves the home page using this strict, centralized priority:

1. Configured `home = "..."` from `.terminalwiki.toml`
2. `index.md`
3. `README.md`
4. `Home.md`
5. `home.md`
6. Automatic generated wiki overview listing all indexed pages
