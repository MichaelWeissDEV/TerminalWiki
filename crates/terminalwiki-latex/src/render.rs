//! Pure adapter from native LaTeX AST to TerminalWiki's shared render AST.

use crate::ast::{Block as AstBlock, Document as AstDocument, Inline as AstInline, ListItem};
use terminalwiki_render::document::{Block, Document, Inline};

pub fn adapt_document(document: &AstDocument) -> Document {
    Document {
        blocks: document.blocks.iter().map(adapt_block).collect(),
    }
}

fn adapt_block(block: &AstBlock) -> Block {
    match block {
        AstBlock::Heading { level, content, .. } => {
            Block::Heading((*level).into(), adapt_inlines(content))
        }
        AstBlock::Paragraph { content, .. } => Block::Paragraph(adapt_inlines(content)),
        AstBlock::Math { source, .. } => Block::Math(source.clone()),
        AstBlock::List { ordered, items, .. } => Block::List {
            ordered: *ordered,
            start: 1,
            items: items.iter().map(adapt_list_item).collect(),
        },
        AstBlock::Quote { blocks, .. } => {
            Block::BlockQuote(blocks.iter().map(adapt_block).collect())
        }
        AstBlock::Code {
            language, source, ..
        } => Block::CodeBlock {
            language: language.clone(),
            content: source.clone(),
        },
        AstBlock::Image { source, alt, .. } => Block::Image {
            alt: plain_text(alt),
            url: source.clone(),
        },
        AstBlock::Unsupported { source, reason, .. } => Block::CodeBlock {
            language: Some(format!("latex: {reason}")),
            content: source.clone(),
        },
    }
}

fn adapt_list_item(item: &ListItem) -> Vec<Block> {
    item.blocks.iter().map(adapt_block).collect()
}
fn adapt_inlines(inlines: &[AstInline]) -> Vec<Inline> {
    inlines.iter().map(adapt_inline).collect()
}
fn adapt_inline(inline: &AstInline) -> Inline {
    match inline {
        AstInline::Text { text, .. } => Inline::Text(text.clone()),
        AstInline::Bold { content, .. } => Inline::Bold(adapt_inlines(content)),
        AstInline::Italic { content, .. } => Inline::Italic(adapt_inlines(content)),
        AstInline::Code { source, .. } => Inline::Code(source.clone()),
        AstInline::Link { target, label, .. } => Inline::Link {
            target: target.clone(),
            text: adapt_inlines(label),
        },
        AstInline::Reference { target, .. } => Inline::WikiLink {
            target: target.clone(),
            label: Some(target.clone()),
        },
        AstInline::Math { source, .. } => Inline::Math(source.clone()),
    }
}
fn plain_text(inlines: &[AstInline]) -> String {
    inlines
        .iter()
        .map(|inline| match inline {
            AstInline::Text { text, .. }
            | AstInline::Code { source: text, .. }
            | AstInline::Math { source: text, .. } => text.clone(),
            AstInline::Bold { content, .. } | AstInline::Italic { content, .. } => {
                plain_text(content)
            }
            AstInline::Link { label, .. } => plain_text(label),
            AstInline::Reference { target, .. } => target.clone(),
        })
        .collect()
}
