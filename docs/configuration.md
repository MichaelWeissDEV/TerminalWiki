# Configuration Reference

TerminalWiki loads configuration from `~/.config/terminalwiki/config.toml` (global) and optional `.terminalwiki.toml` (per-wiki root).

## Example Configuration

```toml
# Default wiki to load when invoked without arguments
default_wiki = "notes"

# Preferred editor ($VISUAL or $EDITOR used if unset)
editor = "nvim"

# Visual theme: "dark" | "light" | "mono"
theme = "dark"

[render]
# Maximum line width for rendered markdown
max_content_width = 100

# Show line numbers in code blocks
line_numbers = true

# Graphics backend: "auto" | "kitty" | "iterm2" | "sixel" | "unicode" | "off"
graphics = "auto"

# Render LaTeX math formulas
math = true

[index]
# Index code files
code = true

# Index hidden files
hidden = false

# Maximum file size to index for full-text search (in bytes)
max_file_size = 2097152

# Path ignore patterns
ignore = ["target", ".git", "node_modules"]

[[wiki]]
name = "notes"
path = "/Users/michaelweiss/Knowledge"
```
