//! Markdown parser producing semantic AST blocks and inlines (spec §55, §56).

use pulldown_cmark::{Event, HeadingLevel, LinkType, Options, Parser, Tag, TagEnd};

use crate::document::{Block, Document, Inline};

/// Parses raw markdown text into a semantic `Document` AST.
pub fn parse_markdown(text: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_MATH);
    // Without this, CommonMark parses `[[Heap]]` as a nested link reference and
    // splits it across several `Event::Text` chunks, so no text-level scanner can
    // ever see a complete `[[...]]` span.
    options.insert(Options::ENABLE_WIKILINKS);

    let parser = Parser::new_ext(text, options);
    let mut doc = Document::default();

    let mut block_stack: Vec<Vec<Block>> = vec![Vec::new()];
    let mut inline_stack: Vec<Vec<Inline>> = vec![Vec::new()];
    // (destination, wikilink pothole flag) — `None` means an ordinary CommonMark link.
    let mut link_dest_stack: Vec<(String, Option<bool>)> = Vec::new();
    let mut image_dest_stack: Vec<String> = Vec::new();

    let mut current_code_block: Option<Option<String>> = None;
    let mut current_code_content = String::new();

    // Table state
    let mut in_table = false;
    let mut in_table_head = false;
    let mut table_headers: Vec<Vec<Inline>> = Vec::new();
    let mut table_rows: Vec<Vec<Vec<Inline>>> = Vec::new();
    let mut current_row: Vec<Vec<Inline>> = Vec::new();

    for event in parser {
        match event {
            // ─── Code Blocks ─────────────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    pulldown_cmark::CodeBlockKind::Fenced(l) => {
                        let l_str = l.to_string();
                        if l_str.is_empty() {
                            None
                        } else {
                            Some(l_str)
                        }
                    }
                    pulldown_cmark::CodeBlockKind::Indented => None,
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

            // ─── Headings ────────────────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                inline_stack.push(Vec::new());
                let _ = level;
            }
            Event::End(TagEnd::Heading(level)) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                let lvl = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                block_stack
                    .last_mut()
                    .unwrap()
                    .push(Block::Heading(lvl, inlines));
            }

            // ─── Paragraphs ──────────────────────────────────────────────────────
            Event::Start(Tag::Paragraph) => {
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Paragraph) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                if !inlines.is_empty() {
                    block_stack
                        .last_mut()
                        .unwrap()
                        .push(Block::Paragraph(inlines));
                }
            }

            // ─── Block Quotes ────────────────────────────────────────────────────
            Event::Start(Tag::BlockQuote(_)) => {
                block_stack.push(Vec::new());
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                let inner = block_stack.pop().unwrap_or_default();
                block_stack
                    .last_mut()
                    .unwrap()
                    .push(Block::BlockQuote(inner));
            }

            // ─── Lists ───────────────────────────────────────────────────────────
            Event::Start(Tag::List(first_item_number)) => {
                block_stack.push(Vec::new());
                let _ = first_item_number;
            }
            Event::End(TagEnd::List(ordered)) => {
                let items = block_stack.pop().unwrap_or_default();
                let list_items: Vec<Vec<Block>> = items.into_iter().map(|b| vec![b]).collect();
                block_stack.last_mut().unwrap().push(Block::List {
                    ordered,
                    start: 1,
                    items: list_items,
                });
            }
            Event::Start(Tag::Item) => {
                block_stack.push(Vec::new());
            }
            Event::End(TagEnd::Item) => {
                let item_blocks = block_stack.pop().unwrap_or_default();
                if let Some(parent) = block_stack.last_mut() {
                    if !item_blocks.is_empty() {
                        parent.extend(item_blocks);
                    }
                }
            }

            // ─── Tables ──────────────────────────────────────────────────────────
            Event::Start(Tag::Table(_)) => {
                in_table = true;
                table_headers.clear();
                table_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                in_table = false;
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
                if !in_table_head && in_table {
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
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) => {
                let pothole = match link_type {
                    LinkType::WikiLink { has_pothole } => Some(has_pothole),
                    _ => None,
                };
                link_dest_stack.push((dest_url.to_string(), pothole));
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Link) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                let (target, pothole) = link_dest_stack.pop().unwrap_or_default();
                let inline = match pothole {
                    // `[[Target|Label]]` — the link body is a distinct display label.
                    Some(true) => Inline::WikiLink {
                        target,
                        label: Some(inlines.iter().map(|i| i.plain_text()).collect::<String>()),
                    },
                    // `[[Target]]` — body and target are the same text.
                    Some(false) => Inline::WikiLink {
                        target,
                        label: None,
                    },
                    None => Inline::Link {
                        target,
                        text: inlines,
                    },
                };
                inline_stack.last_mut().unwrap().push(inline);
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                image_dest_stack.push(dest_url.to_string());
                inline_stack.push(Vec::new());
            }
            Event::End(TagEnd::Image) => {
                let inlines = inline_stack.pop().unwrap_or_default();
                let alt = inlines.iter().map(|i| i.plain_text()).collect::<String>();
                let url = image_dest_stack.pop().unwrap_or_default();
                block_stack
                    .last_mut()
                    .unwrap()
                    .push(Block::Image { alt, url });
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
            Event::InlineMath(m) => {
                inline_stack
                    .last_mut()
                    .unwrap()
                    .push(Inline::Math(m.to_string()));
            }
            Event::DisplayMath(m) => {
                block_stack
                    .last_mut()
                    .unwrap()
                    .push(Block::Math(m.to_string()));
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
    fn test_markdown_link_target_preservation() {
        let md = "[docs](https://example.org)";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        if let Block::Paragraph(inlines) = &doc.blocks[0] {
            assert_eq!(inlines.len(), 1);
            match &inlines[0] {
                Inline::Link { target, text } => {
                    assert_eq!(target, "https://example.org");
                    assert_eq!(text[0].plain_text(), "docs");
                }
                _ => panic!("Expected Inline::Link"),
            }
        } else {
            panic!("Expected paragraph");
        }
    }

    #[test]
    fn test_image_alt_and_url_preservation() {
        let md = "![Heap diagram](images/heap.png)";
        let doc = parse_markdown(md);
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Image { alt, url } => {
                assert_eq!(alt, "Heap diagram");
                assert_eq!(url, "images/heap.png");
            }
            _ => panic!("Expected Block::Image"),
        }
    }

    #[test]
    fn test_all_link_types_and_no_collision() {
        let md = r#"
[docs](https://example.org)
[relative](../foo.md)
[anchor](#heading)
![image](assets/test.png)
[[Heap]]
[[Heap|Heap Exploitation]]
[[security::Heap]]
"#;
        let doc = parse_markdown(md);
        assert!(doc.blocks.len() >= 2);
    }

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
}
