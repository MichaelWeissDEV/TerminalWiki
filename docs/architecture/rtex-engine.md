# RTex: native TeX-engine extraction plan

TerminalWiki's current `terminalwiki-latex` crate is a bounded, read-only
frontend. The long-term implementation is **RTex**, a separate Rust library and
eventually a separately versioned repository that TerminalWiki consumes as a
normal dependency. No remote repository is created or published by this plan.

## Compatibility objective

RTex targets practical LaTeX compatibility in stages: ordinary documents,
packages/macros, Unicode engines, maths, tables, references, page breaking and
eventually PDF/SVG/DVI output. It must not pretend to be a drop-in LuaLaTeX
replacement until compatibility fixtures demonstrate that claim. TerminalWiki
can use its document IR before page-perfect typesetting is complete.

## Layers

1. **Reader frontend (current):** bounded lexer, structural parser, anchors,
   references, include resolver and native terminal math. No expansion or I/O
   beyond explicit root-confined source includes.
2. **RTex token layer:** TeX tokenization with configurable catcodes and exact
   source maps. Tokens are immutable and allocation/resource bounded.
3. **Expansion layer:** macro definitions, arguments, conditionals, registers
   and grouping with recursion/step budgets. Package support is implemented as
   parsers/modules, never arbitrary host-code execution.
4. **TeX layout VM:** boxes/glue/penalties, line/page breaking, fonts and
   backend-independent shipout IR.
5. **Backends:** TerminalWiki reader cells first; then SVG/PDF/DVI emitters as
   explicit library calls. Generated artifacts are caller-owned and never
   written next to a source document by default.
6. **Compatibility corpus:** versioned TeX/LaTeX fixtures and expected AST,
   layout or output snapshots, including real-world documents where licensing
   permits redistribution.

## Capabilities, not hidden execution

RTex distinguishes TeX language compatibility from host privileges. File reads,
writes, shell escape, Lua callbacks, network access, fonts, time/randomness and
environment variables are injected capabilities. The default TerminalWiki
profile permits only root-confined reads required for an explicitly requested
document and denies writes, shell execution, Lua and network access. A future
CLI compile mode may opt into an audited policy; it must never be enabled just
by opening a document.

## Extraction boundary

`terminalwiki-latex` owns TerminalWiki adapters only: wiki containment,
`DocumentView`/render conversion, diagnostics presentation and TUI navigation.
RTex owns TeX tokens, AST, expansion and layout. The current public parser API
is deliberately data-oriented so it can migrate to `rtex-reader` without
changing CLI/TUI call sites.
