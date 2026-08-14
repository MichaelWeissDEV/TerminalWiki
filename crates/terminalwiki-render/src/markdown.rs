//! Markdown parsing into semantic Document AST using pulldown-cmark (spec §55, §60).

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::document::{Block, Document, Inline};

/// Parses markdown text into a structured semantic `Document`.
pub fn parse_markdown(text: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);

    let parser = Parser::new_ext(text, options);
    let mut doc = Document::default();

    let mut inline_stack: Vec<Vec<Inline>> = vec![Vec::new()];
    let mut block_stack: Vec<Vec<Block>> = vec![Vec::new()];

    let mut current_heading_level: Option<usize> = None;
    let mut current_code_block: Option<Option<String>> = None;
    let mut current_code_content = String::new();

    let mut table_headers: Vec<Vec<Inline>> = Vec::new();
    let mut table_rows: Vec<Vec<Vec<Inline>>> = Vec::new();
    let mut current_row: Vec<Vec<Inline>> = Vec::new();
    let mut in_table_head = false;

    for event in parser {
        match event {
            // ─── Headings ────────────────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                current_heading_level = Some(level as usize);
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = current_heading_level.take() {
                    let inlines = inline_stack.pop().unwrap_or_default();
                    block_stack
                        .last_mut()
                        .unwrap()
                        .push(Block::Heading(level, inlines));
                }
            }

            // ─── Paragraphs ──────────────────────────────────────────────────────
            Event::Start(Tag::Paragraph) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Paragraph) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                if !inlines.is_empty() {
                    // Check if paragraph starts with callout indicator e.g. [!NOTE]
                    let plain = inlines.iter().map(|i| i.plain_text()).collect::<String>();
                    if let Some(rest) = plain.strip_prefix("[!") {
                        if let Some((kind, _)) = rest.split_once(']') {
                            let kind_upper = kind.to_ascii_uppercase();
                            if ["NOTE", "TIP", "WARNING", "CAUTION", "IMPORTANT"]
                                .contains(&kind_upper.as_str())
                            {
                                block_stack.last_mut().unwrap().push(Block::Callout {
                                    kind: kind_upper,
                                    title: None,
                                    content: vec![Block::Paragraph(inlines)],
                                });
                                continue;
                            }
                        }
                    }
                    block_stack
                        .last_mut()
                        .unwrap()
                        .push(Block::Paragraph(inlines));
                }
            }

            // ─── Code Blocks ─────────────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => {
                        let s = l.trim().to_string();
                        if s.is_empty() {
                            None
                        } else {
                            Some(s)
                        }
                    }
                    CodeBlockKind::Indented => None,
                };
                current_code_block = Some(lang);
                current_code_content.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(lang) = current_code_block.take() {
                    block_stack.last_mut().unwrap().push(Block::CodeBlock {
                        language: lang,
                        content: std::mem::take(&mut current_code_content),
                    });
                }
            }

            // ─── Block Quotes ────────────────────────────────────────────────────
            Event::Start(Tag::BlockQuote(_)) => {
                block_stack.push(Vec::new());
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                let blocks = block_stack.pop().unwrap_or_default();
                block_stack
                    .last_mut()
                    .unwrap()
                    .push(Block::BlockQuote(blocks));
            }

            // ─── Lists ───────────────────────────────────────────────────────────
            Event::Start(Tag::List(first_num)) => {
                block_stack.push(Vec::new());
                let ordered = first_num.is_some();
                let start = first_num;
                // Store list metadata in list items
                let _ = (ordered, start);
            }
            Event::End(TagEnd::List(ordered)) => {
                let items = block_stack.pop().unwrap_or_default();
                let item_blocks: Vec<Vec<Block>> = items.into_iter().map(|b| vec![b]).collect();
                block_stack.last_mut().unwrap().push(Block::List {
                    ordered,
                    start: None,
                    items: item_blocks,
                });
            }
            Event::Start(Tag::Item) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Item) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                if !inlines.is_empty() {
                    block_stack
                        .last_mut()
                        .unwrap()
                        .push(Block::Paragraph(inlines));
                }
            }

            // ─── Tables ──────────────────────────────────────────────────────────
            Event::Start(Tag::Table(_)) => {
                table_headers.clear();
                table_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                block_stack.last_mut().unwrap().push(Block::Table {
                    headers: std::mem::take(&mut table_headers),
                    rows: std::mem::take(&mut table_rows),
                });
            }
            Event::Start(Tag::TableHead) => {
                in_table_head = true;
                current_row.clear();
            }
            Event::End(TagEnd::TableHead) => {
                in_table_head = false;
                table_headers = std::mem::take(&mut current_row);
            }
            Event::Start(Tag::TableRow) => {
                current_row.clear();
            }
            Event::End(TagEnd::TableRow) => {
                if !in_table_head {
                    table_rows.push(std::mem::take(&mut current_row));
                }
            }
            Event::Start(Tag::TableCell) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::TableCell) => {
                let cell_inlines = inline_stack.pop().unwrap_or_default();
                current_row.push(cell_inlines);
            }

            // ─── Inlines ─────────────────────────────────────────────────────────
            Event::Start(Tag::Emphasis) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Emphasis) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Italic(inlines));
            }
            Event::Start(Tag::Strong) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Strong) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                inline_stack.last_mut().unwrap().push(Inline::Bold(inlines));
            }
            Event::Start(Tag::Strikethrough) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Strikethrough) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Strike(inlines));
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                inline_stack.push(Vec::new());
                // Save link target for tag end
                let target = dest_url.to_string();
                let _ = target;
            }
            Event::End(TagEnd::Link) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                let plain_target = inlines.iter().map(|i| i.plain_text()).collect::<String>();
                inline_stack.last_mut().unwrap().push(Inline::Link {
                    target: plain_target,
                    text: inlines,
                });
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                inline_stack.push(Vec::new());
                let url = dest_url.to_string();
                let _ = url;
            }
            Event::End(TagEnd::Image) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                let alt = inlines.iter().map(|i| i.plain_text()).collect::<String>();
                block_stack.last_mut().unwrap().push(Block::Image {
                    alt: alt.clone(),
                    url: alt,
                });
            }

            // ─── Text & Code ─────────────────────────────────────────────────────
            Event::Text(t) => {
                if current_code_block.is_some() {
                    current_code_content.push_str(&t);
                } else {
                    parse_wiki_links_in_text(&t, inline_stack.last_mut().unwrap());
                }
            }
            Event::Code(t) => {
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Code(t.to_string()));
            }
            Event::Rule => {
                block_stack.last_mut().unwrap().push(Block::HorizontalRule);
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x] " } else { "[ ] " };
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Text(marker.to_string()));
            }
            Event::FootnoteReference(name) => {
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Footnote(format!("[^{name}]")));
            }
            Event::SoftBreak | Event::HardBreak => {
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Text(" ".to_string()));
            }
            _ => {}
        }
    }

    doc.blocks = block_stack.pop().unwrap_or_default();
    doc
}

/// Parses inline `[[WikiLink]]` occurrences from raw text chunks.
fn parse_wiki_links_in_text(text: &str, inlines: &mut Vec<Inline>) {
    let mut cursor = 0;
    while let Some(start) = text[cursor..].find("[[") {
        let abs_start = cursor + start;
        if abs_start > cursor {
            inlines.push(Inline::Text(text[cursor..abs_start].to_string()));
        }
        if let Some(end) = text[abs_start + 2..].find("]]") {
            let abs_end = abs_start + 2 + end;
            let inner = &text[abs_start + 2..abs_end];
            if let Some((target, label)) = inner.split_once('|') {
                inlines.push(Inline::WikiLink {
                    target: target.trim().to_string(),
                    label: Some(label.trim().to_string()),
                });
            } else {
                inlines.push(Inline::WikiLink {
                    target: inner.trim().to_string(),
                    label: None,
                });
            }
            cursor = abs_end + 2;
        } else {
            inlines.push(Inline::Text(text[abs_start..].to_string()));
            cursor = text.len();
            break;
        }
    }
    if cursor < text.len() {
        inlines.push(Inline::Text(text[cursor..].to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Block, Inline};

    #[test]
    fn test_heading_and_paragraph() {
        let md = "# Memory Management\n\nExploiting the glibc heap.";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Heading(1, inlines) => {
                assert_eq!(inlines[0], Inline::Text("Memory Management".into()));
            }
            _ => panic!("Expected Heading(1, ..)"),
        }
        match &doc.blocks[1] {
            Block::Paragraph(inlines) => {
                assert_eq!(
                    inlines[0],
                    Inline::Text("Exploiting the glibc heap.".into())
                );
            }
            _ => panic!("Expected Paragraph"),
        }
    }

    #[test]
    fn test_bold_italic_and_nested() {
        let md = "Normal **bold text** and *italic text* and ***bold italic***.";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        if let Block::Paragraph(inlines) = &doc.blocks[0] {
            assert!(inlines.iter().any(|i| matches!(i, Inline::Bold(_))));
            assert!(inlines.iter().any(|i| matches!(i, Inline::Italic(_))));
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn test_inline_code_and_links() {
        let md = "Call `malloc(size)` or visit [docs](https://example.org).";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        if let Block::Paragraph(inlines) = &doc.blocks[0] {
            assert_eq!(inlines[1], Inline::Code("malloc(size)".into()));
            assert!(
                matches!(&inlines[3], Inline::Link { target, .. } if target == "https://example.org")
            );
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn test_wiki_links_and_aliases() {
        let md = "See [[Heap]] and [[Heap|Heap Exploitation]] for details.";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        if let Block::Paragraph(inlines) = &doc.blocks[0] {
            assert_eq!(inlines[0], Inline::Text("See ".into()));
            assert_eq!(
                inlines[1],
                Inline::WikiLink {
                    target: "Heap".into(),
                    label: None,
                }
            );
            assert_eq!(inlines[2], Inline::Text(" and ".into()));
            assert_eq!(
                inlines[3],
                Inline::WikiLink {
                    target: "Heap".into(),
                    label: Some("Heap Exploitation".into()),
                }
            );
            assert_eq!(inlines[4], Inline::Text(" for details.".into()));
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn test_lists_and_tasks() {
        let md = "- [ ] Task 1\n- [x] Task 2\n- Item 3";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        assert!(matches!(&doc.blocks[0], Block::List { .. }));
    }

    #[test]
    fn test_code_blocks() {
        let md = "```rust\nfn main() {\n    println!(\"hi\");\n}\n```";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::CodeBlock { language, content } => {
                assert_eq!(language.as_deref(), Some("rust"));
                assert!(content.contains("println!(\"hi\");"));
            }
            _ => panic!("Expected CodeBlock"),
        }
    }

    #[test]
    fn test_callouts() {
        let md = "> [!NOTE]\n> Important security note.";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::BlockQuote(inner) | Block::Callout { content: inner, .. } => {
                assert!(!inner.is_empty());
            }
            _ => panic!("Expected Callout or BlockQuote"),
        }
    }

    #[test]
    fn test_tables() {
        let md = "| Header A | Header B |\n|---|---|\n| Cell 1 | Cell 2 |";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Table { headers, rows } => {
                assert_eq!(headers.len(), 2);
                assert_eq!(rows.len(), 1);
            }
            _ => panic!("Expected Table"),
        }
    }

    #[test]
    fn test_complex_combination() {
        let md =
            "# Heap\n\nSee **[[Malloc|the allocator]]** and [`malloc()`](https://example.invalid).";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::Heading(1, _)));
        assert!(matches!(&doc.blocks[1], Block::Paragraph(_)));
    }
}
