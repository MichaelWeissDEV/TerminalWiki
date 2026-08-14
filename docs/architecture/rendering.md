# Rendering Architecture

## Pipeline Overview

```text
Source Content (Markdown / Code)
              ↓
  Classification (filetype)
              ↓
      AST Construction
   (Document / CodeView)
              ↓
           Layout
 (Width Wrapping / Alignment)
              ↓
       RenderedDocument
(Lines of Styled Spans + Links)
              ↓
      Terminal Output
```

## First-Class Source Code Rendering

Source files (`*.rs`, `*.c`, `*.py`, `*.go`, etc.) bypass the Markdown parser and flow directly into the `CodeRenderer`:
- Syntax highlighting via `syntect` using base16 palettes.
- Complete preservation of whitespace and column alignment.
- Aligned line numbers and line range highlighting (`tw code file.rs:10-20`).

## Semantic Document AST

Markdown is parsed using `pulldown-cmark` into a clean semantic `Document` containing:
- `Block`: Headings, Paragraphs, CodeBlocks, Lists, TaskLists, Tables, BlockQuotes, Callouts, HorizontalRules, Images, Math.
- `Inline`: Text, Bold, Italic, Strike, Code, Links, WikiLinks `[[...]]`, Footnotes, Math.
- Links are recorded as first-class objects with visual positions, enabling interactive `Tab`/`Enter` link following in TUI mode.
