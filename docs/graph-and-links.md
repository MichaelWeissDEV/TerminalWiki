# Knowledge Graph and Links

## WikiLink Syntax

TerminalWiki supports standard Obsidian-style WikiLinks:

- Direct page link: `[[Target Page]]`
- Alias / Display text: `[[Target Page|Custom Label]]`
- Heading anchor: `[[Target Page#Section Name]]`
- Cross-wiki link: `[[@otherwiki:Target Page]]`
- Source code link: `[[file:src/main.rs#L30-L50]]`

## Graph Analysis

### Backlinks (`tw backlinks PAGE`)
Identifies all files containing links pointing to the target page.

### Related Pages (`tw related PAGE`)
Deterministic relatedness scoring algorithm:
- Direct outgoing/incoming links: +10 / +8
- Shared tags: +5 per tag
- Same directory: +3
- Common neighbors: +2 each

### Graph Export (`tw graph --format dot`)
Generates Graphviz DOT graphs for external visualizers:

```bash
tw graph --format dot | dot -Tsvg -o graph.svg
```
