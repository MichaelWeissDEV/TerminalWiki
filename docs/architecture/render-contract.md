# Render contract (v0.2 beta integration)

This document freezes the minimal cross-crate reader contract before optional
LaTeX, media, diagram and TUI work is integrated. It extends the existing
`terminalwiki-render` types; it does **not** replace the working Markdown,
plain-text, ANSI or crossterm paths in one step.

## Existing compatibility boundary

- `terminalwiki_render::document::Document` and `Block` remain the parser-facing
  representation for the existing Markdown renderer.
- `RenderedDocument { lines, headings, links }` remains the immediate text-cell
  output consumed by the CLI and current crossterm TUI.
- `terminalwiki_core::link::LinkTarget` remains the single link-resolution
  authority. New renderers must not create a second resolver.
- Path containment remains `Wiki::resolve_within`; content providers receive a
  wiki-relative path and never an unrestricted filesystem path.

## Additive IR

New functionality is introduced behind additive types in
`terminalwiki-render::document` (or a small sibling module) with conversion to
`RenderedDocument` until all consumers can accept richer output.

```text
DocumentId       = { wiki: String, relative_path: PathBuf }
DocumentKind     = Markdown | Latex | Pdf | Image | Code | Text | Unknown
SourceLocation   = { file: PathBuf, byte_start?, byte_end?, line?, column? }
Anchor           = { id: String, title: String, location?: SourceLocation }
NavigationTarget = Document(DocumentId) | Anchor(DocumentId, Anchor)
DocumentView     = { title, kind, blocks, anchors, links, outline, warnings, assets }
```

`DocumentView` is a read-only semantic view. A document parser may omit source
offsets when its backend cannot provide them, but must preserve stable anchors
and controlled warnings for unsupported input.

```text
RenderBlock = Text | Heading | Code | Math | Table | Image | Diagram | PdfPage | Unsupported
RenderRequest = { width_cells, height_cells?, theme, terminal_capabilities, content_revision }
RenderOutput = { text_cells, graphics_placements, links_hit_regions, visible_anchors }
```

`text_cells` is represented initially by `RenderedDocument`; graphics placements
are empty for the current text backend. Renderers must use display-cell widths,
not byte or `char` counts. `--plain` and non-TTY calls must emit text only and
must not contain terminal graphics control sequences.

## Math and media boundary

The native math implementation returns a bounded 2D layout:

```text
MathLayout = { lines: Vec<String>, width_cells, height_cells, baseline, warnings }
```

Inline math is converted to spans and display math to block lines. Unsupported
commands remain visible as sanitized source plus a warning; they are never
silently dropped or replaced solely by an opaque formula placeholder.

Media is supplied by traits rather than parser-side subprocesses:

```text
MediaProvider / ImageProvider: inspect and decode bounded local assets
PdfProvider: open, page_count, metadata, outline, extract_text?, raster_page?
TerminalCapabilities: graphics protocol only after real TTY/capability checks
```

Providers return in-memory bytes/cells and cancellation-aware errors. They must
not download content, execute document instructions, or write beside a source
document.

## Invariants and rollout

1. One `LinkTarget` resolver, one Unicode-width implementation, and one root
   containment boundary are used by CLI, TUI and every document kind.
2. Parser and layout limits are explicit (input bytes, tokens, AST depth, image
   pixels, PDF page/cache budget); unknown input fails visibly without panic.
3. TUI link hit regions and anchors refer to render-cell coordinates and are
   discarded on a content-revision change.
4. Existing `Document`/`RenderedDocument` adapters are retained while each new
   crate is independently buildable. A consumer migrates only with a contract
   test covering source locations, anchors, widths and no-TTY output.

The contract is frozen for the beta implementation. Changes require updating
this document and the adapter tests before cross-crate integration.
