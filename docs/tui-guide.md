# TUI Guide

TerminalWiki's interactive TUI (`tw tui`) provides frictionless navigation without heavy desktop bloat.

## Keybindings

| Key | Action |
| --- | --- |
| `j` / `Down` | Scroll down one line |
| `k` / `Up` | Scroll up one line |
| `h` / `l` | Scroll left / right horizontally |
| `Ctrl+d` / `Ctrl+u` | Half-page scroll down / up |
| `g` / `G` | Jump to top / bottom |
| `Tab` / `Shift+Tab` | Cycle forward / backward through document links |
| `Enter` | Follow selected link or open selected item |
| `f` / `Ctrl+p` | Open inline fuzzy finder |
| `o` | Open document outline view |
| `b` | Open backlinks list |
| `:` | Open Command Palette |
| `e` | Open current document in `$EDITOR` |
| `Ctrl+o` / `Ctrl+i` | Navigate backward / forward in history |
| `/` | In-page text search |
| `?` | Toggle help modal |
| `q` / `Esc` | Return to document / quit |

## Command Palette (`:`)

Pressing `:` brings up the command bar:
- `:open <page>`: Resolves and loads target page.
- `:search <query>`: Opens fuzzy finder pre-filled with query.
- `:backlinks`: Opens backlinks inspector.
- `:outline`: Shows table of contents.
- `:reload`: Reloads current file from disk.
- `:wiki <name>`: Switches to a different configured wiki.
- `:edit`: Launches editor for current page.
- `:quit` or `:q`: Exits the TUI.
