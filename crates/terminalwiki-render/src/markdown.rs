use pulldown_cmark::{Parser, Event, Tag, TagEnd, Options, CodeBlockKind};
use crate::document::{Document, Block};

pub fn parse_markdown(text: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    // pulldown-cmark doesn't have math natively without extensions but we can parse simple math from text if needed, or assume text contains it.
    // Spec says use simple math fallback.
    let parser = Parser::new_ext(text, options);
    
    let mut doc = Document::default();
    let mut current_text = String::new();
    let mut in_heading = None;
    let mut _in_paragraph = false;
    let mut in_code_block = None;
    
    // Very basic parsing for now.
    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = Some(level as usize);
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = in_heading {
                    doc.blocks.push(Block::Heading(level, current_text.clone()));
                    current_text.clear();
                }
                in_heading = None;
            }
            Event::Start(Tag::Paragraph) => {
                _in_paragraph = true;
            }
            Event::End(TagEnd::Paragraph) => {
                if current_text.starts_with("[!NOTE]") {
                    doc.blocks.push(Block::Callout("NOTE".to_string(), current_text[7..].trim().to_string()));
                } else {
                    doc.blocks.push(Block::Paragraph(current_text.clone()));
                }
                current_text.clear();
                _in_paragraph = false;
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(l) => Some(l.to_string()),
                    _ => None,
                };
                in_code_block = Some(lang);
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(lang) = in_code_block.take() {
                    doc.blocks.push(Block::CodeBlock(lang, current_text.clone()));
                    current_text.clear();
                }
            }
            Event::Text(t) => {
                current_text.push_str(&t);
            }
            Event::Code(t) => {
                current_text.push('`');
                current_text.push_str(&t);
                current_text.push('`');
            }
            _ => {}
        }
    }
    
    doc
}
