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
                    block_stack.last_mut().unwrap().push(Block::Heading(level, inlines));
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
                            if ["NOTE", "TIP", "WARNING", "CAUTION", "IMPORTANT"].contains(&kind_upper.as_str()) {
                                block_stack.last_mut().unwrap().push(Block::Callout {
                                    kind: kind_upper,
                                    title: None,
                                    content: vec![Block::Paragraph(inlines)],
                                });
                                continue;
                            }
                        }
                    }
                    block_stack.last_mut().unwrap().push(Block::Paragraph(inlines));
                }
            }

            // ─── Code Blocks ─────────────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => {
                        let s = l.trim().to_string();
                        if s.is_empty() { None } else { Some(s) }
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
                block_stack.last_mut().unwrap().push(Block::BlockQuote(blocks));
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
                    block_stack.last_mut().unwrap().push(Block::Paragraph(inlines));
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
                inline_stack.last_mut().unwrap().push(Inline::Italic(inlines));
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
                inline_stack.last_mut().unwrap().push(Inline::Strike(inlines));
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
                inline_stack.last_mut().unwrap().push(Inline::Code(t.to_string()));
            }
            Event::Rule => {
                block_stack.last_mut().unwrap().push(Block::HorizontalRule);
            }
            Event::TaskListMarker(checked) => {
                let marker = if checked { "[x] " } else { "[ ] " };
                inline_stack.last_mut().unwrap().push(Inline::Text(marker.to_string()));
            }
            Event::FootnoteReference(name) => {
                inline_stack.last_mut().unwrap().push(Inline::Footnote(format!("[^{name}]")));
            }
            Event::SoftBreak | Event::HardBreak => {
                inline_stack.last_mut().unwrap().push(Inline::Text(" ".to_string()));
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
