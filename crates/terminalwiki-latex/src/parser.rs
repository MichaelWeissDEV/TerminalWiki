//! Bounded, non-executing structural parser for the reader subset of LaTeX.
//!
//! This is deliberately not TeX: it never expands macros, opens an include,
//! follows a URL, or invokes an external program.  Input outside the small
//! reader subset is retained in [`Block::Unsupported`] and diagnosed.

use crate::ast::{
    Anchor, Block, Document, Inline, ListItem, OutlineEntry, ReferenceKind, SourceSpan, Warning,
    WarningKind,
};
use crate::lexer::{LexError, LexWarning, Lexer, LexerLimits, Token, TokenKind};
use std::fmt;

/// Bounds applied in addition to the lexer's bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParserLimits {
    pub lexer: LexerLimits,
    pub max_blocks: usize,
    pub max_inlines_per_block: usize,
    pub max_nesting: usize,
    pub max_warnings: usize,
}

impl Default for ParserLimits {
    fn default() -> Self {
        Self {
            lexer: LexerLimits::default(),
            max_blocks: 100_000,
            max_inlines_per_block: 100_000,
            max_nesting: 64,
            max_warnings: 4_096,
        }
    }
}

/// A bounded parsing failure. Unsupported or malformed LaTeX is not an error:
/// it is returned as visible source plus a warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Lex(LexError),
    LimitExceeded {
        resource: &'static str,
        maximum: usize,
    },
    AllocationFailed {
        resource: &'static str,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => write!(f, "could not lex LaTeX: {error}"),
            Self::LimitExceeded { resource, maximum } => {
                write!(f, "{resource} exceeds its {maximum} item limit")
            }
            Self::AllocationFailed { resource } => write!(f, "could not allocate {resource}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(value: LexError) -> Self {
        Self::Lex(value)
    }
}

/// Parses with [`ParserLimits::default`].
pub fn parse_tex_document(source: &str) -> Result<Document, ParseError> {
    Parser::default().parse(source)
}

/// Reusable parser with explicit resource bounds.
#[derive(Clone, Copy, Debug)]
pub struct Parser {
    limits: ParserLimits,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new(ParserLimits::default())
    }
}

impl Parser {
    pub const fn new(limits: ParserLimits) -> Self {
        Self { limits }
    }

    pub const fn limits(&self) -> ParserLimits {
        self.limits
    }

    pub fn parse(&self, source: &str) -> Result<Document, ParseError> {
        let lexed = Lexer::new(self.limits.lexer).lex(source)?;
        let mut state = State {
            source,
            tokens: &lexed.tokens,
            limits: self.limits,
            document: Document::new(),
        };
        for warning in lexed.warnings {
            let span = match warning {
                LexWarning::NulByte { span } => Some(SourceSpan::new(span.start, span.end)),
            };
            state.warning(
                WarningKind::MalformedSyntax,
                "NUL byte retained as literal text",
                span,
            )?;
        }
        state.blocks(0, state.tokens.len(), 0)?;
        Ok(state.document)
    }
}

struct State<'a> {
    source: &'a str,
    tokens: &'a [Token],
    limits: ParserLimits,
    document: Document,
}

impl<'a> State<'a> {
    fn blocks(&mut self, start: usize, end: usize, depth: usize) -> Result<(), ParseError> {
        if depth > self.limits.max_nesting {
            return Err(ParseError::LimitExceeded {
                resource: "nesting",
                maximum: self.limits.max_nesting,
            });
        }
        let mut cursor = start;
        let mut paragraph_start = start;
        while cursor < end {
            if self.blank_at(cursor) {
                self.paragraph(paragraph_start, cursor)?;
                cursor += 1;
                paragraph_start = cursor;
                continue;
            }
            if let Some(name) = self.command(cursor) {
                if matches!(
                    name,
                    "section" | "subsection" | "subsubsection" | "paragraph" | "title" | "author"
                ) {
                    self.paragraph(paragraph_start, cursor)?;
                    let next = self.heading_or_metadata(cursor, name)?;
                    cursor = next.min(end);
                    paragraph_start = cursor;
                    continue;
                }
                if name == "label" {
                    self.paragraph(paragraph_start, cursor)?;
                    let next = self.label(cursor)?;
                    cursor = next.min(end);
                    paragraph_start = cursor;
                    continue;
                }
                if name == "begin" {
                    self.paragraph(paragraph_start, cursor)?;
                    let next = self.environment(cursor, end, depth)?;
                    cursor = next.min(end);
                    paragraph_start = cursor;
                    continue;
                }
                if is_dangerous(name) || !is_inline(name) {
                    self.paragraph(paragraph_start, cursor)?;
                    let next = self.unsupported_command(cursor, name)?;
                    cursor = next.min(end);
                    paragraph_start = cursor;
                    continue;
                }
            }
            cursor += 1;
        }
        self.paragraph(paragraph_start, end)
    }

    fn heading_or_metadata(&mut self, at: usize, command: &str) -> Result<usize, ParseError> {
        let Some(group) = self.group_after(at) else {
            return self.malformed_unsupported(at, command, "missing braced argument");
        };
        let content = self.inlines(group.inner_start, group.inner_end, 0)?;
        let span = self.span(at, group.after);
        match command {
            "title" => self.push_block(Block::Paragraph {
                content: vec![Inline::Bold { content, span }],
                span,
            })?,
            "author" => self.push_block(Block::Paragraph { content, span })?,
            _ => {
                let level = match command {
                    "section" => 1,
                    "subsection" => 2,
                    "subsubsection" => 3,
                    _ => 4,
                };
                let index = self.document.blocks.len();
                self.push_block(Block::Heading {
                    level,
                    content: content.clone(),
                    anchor: None,
                    span,
                })?;
                self.push_outline(OutlineEntry {
                    level,
                    title: content,
                    anchor: None,
                    block_index: index,
                    span,
                })?;
            }
        }
        Ok(group.after)
    }

    fn label(&mut self, at: usize) -> Result<usize, ParseError> {
        let Some(group) = self.group_after(at) else {
            return self.malformed_unsupported(at, "label", "missing braced label");
        };
        let name = self.text(group.inner_start, group.inner_end)?;
        let span = self.span(at, group.after);
        let block_index = self.document.blocks.len().checked_sub(1);
        if let Some(index) = block_index {
            if let Some(Block::Heading { anchor, .. }) = self.document.blocks.get_mut(index) {
                *anchor = Some(name.clone());
            }
            if let Some(entry) = self
                .document
                .outline
                .iter_mut()
                .rev()
                .find(|entry| entry.block_index == index)
            {
                entry.anchor = Some(name.clone());
            }
        }
        self.push_anchor(Anchor {
            name,
            block_index,
            span,
        })?;
        Ok(group.after)
    }

    fn environment(&mut self, at: usize, bound: usize, depth: usize) -> Result<usize, ParseError> {
        let Some(open) = self.group_after(at) else {
            return self.malformed_unsupported(at, "begin", "missing environment name");
        };
        let name = self.text(open.inner_start, open.inner_end)?;
        let Some((inner_end, after)) = self.find_environment_end(&name, open.after, bound) else {
            self.warning(
                WarningKind::MalformedSyntax,
                "environment has no matching \\end",
                Some(self.span(at, open.after)),
            )?;
            return self.unsupported_span(at, bound, "unterminated environment");
        };
        let span = self.span(at, after);
        match name.as_str() {
            "equation" | "align" | "align*" | "equation*" => self.push_block(Block::Math {
                source: self.text(open.after, inner_end)?,
                span,
            })?,
            "verbatim" => self.push_block(Block::Code {
                language: None,
                source: self.text(open.after, inner_end)?,
                span,
            })?,
            "quote" => {
                let mut nested = State {
                    source: self.source,
                    tokens: self.tokens,
                    limits: self.limits,
                    document: Document::new(),
                };
                nested.blocks(open.after, inner_end, depth + 1)?;
                for warning in nested.document.warnings {
                    self.push_warning(warning)?;
                }
                self.push_block(Block::Quote {
                    blocks: nested.document.blocks,
                    span,
                })?;
            }
            "itemize" | "enumerate" => {
                self.list(open.after, inner_end, name == "enumerate", span, depth + 1)?
            }
            _ => {
                self.warning(
                    WarningKind::UnsupportedEnvironment,
                    "unsupported LaTeX environment retained as source",
                    Some(span),
                )?;
                self.push_block(Block::Unsupported {
                    source: self.text(at, after)?,
                    reason: format!("unsupported environment '{name}'"),
                    span,
                })?;
            }
        }
        Ok(after)
    }

    fn list(
        &mut self,
        start: usize,
        end: usize,
        ordered: bool,
        span: SourceSpan,
        depth: usize,
    ) -> Result<(), ParseError> {
        let mut items = Vec::new();
        let mut item_start = None;
        let mut cursor = start;
        while cursor < end {
            if self.command(cursor) == Some("item") {
                if let Some(first) = item_start {
                    self.list_item(&mut items, first, cursor, depth)?;
                }
                item_start = Some(cursor + 1);
            }
            cursor += 1;
        }
        if let Some(first) = item_start {
            self.list_item(&mut items, first, end, depth)?;
        }
        if items.is_empty() {
            self.warning(
                WarningKind::MalformedSyntax,
                "list has no \\item",
                Some(span),
            )?;
        }
        self.push_block(Block::List {
            ordered,
            items,
            span,
        })
    }

    fn list_item(
        &mut self,
        items: &mut Vec<ListItem>,
        start: usize,
        end: usize,
        depth: usize,
    ) -> Result<(), ParseError> {
        let mut child = State {
            source: self.source,
            tokens: self.tokens,
            limits: self.limits,
            document: Document::new(),
        };
        child.blocks(start, end, depth)?;
        for warning in child.document.warnings {
            self.push_warning(warning)?;
        }
        items
            .try_reserve(1)
            .map_err(|_| ParseError::AllocationFailed {
                resource: "list items",
            })?;
        items.push(ListItem {
            blocks: child.document.blocks,
            span: self.span(start, end),
        });
        Ok(())
    }

    fn paragraph(&mut self, start: usize, end: usize) -> Result<(), ParseError> {
        if start >= end || self.only_ignored(start, end) {
            return Ok(());
        }
        let content = self.inlines(start, end, 0)?;
        if !content.is_empty() {
            self.push_block(Block::Paragraph {
                content,
                span: self.span(start, end),
            })?;
        }
        Ok(())
    }

    fn inlines(
        &mut self,
        start: usize,
        end: usize,
        depth: usize,
    ) -> Result<Vec<Inline>, ParseError> {
        if depth > self.limits.max_nesting {
            return Err(ParseError::LimitExceeded {
                resource: "inline nesting",
                maximum: self.limits.max_nesting,
            });
        }
        let mut output = Vec::new();
        let mut cursor = start;
        while cursor < end {
            if self.command(cursor) == Some("emph")
                || self.command(cursor) == Some("textbf")
                || self.command(cursor) == Some("texttt")
            {
                let name = self.command(cursor).unwrap_or("");
                if let Some(group) = self.group_after(cursor) {
                    let span = self.span(cursor, group.after);
                    let content = self.inlines(group.inner_start, group.inner_end, depth + 1)?;
                    let inline = match name {
                        "emph" => Inline::Italic { content, span },
                        "textbf" => Inline::Bold { content, span },
                        _ => Inline::Code {
                            source: self.text(group.inner_start, group.inner_end)?,
                            span,
                        },
                    };
                    self.push_inline(&mut output, inline)?;
                    cursor = group.after;
                    continue;
                }
            }
            if self.command(cursor) == Some("ref") || self.command(cursor) == Some("eqref") {
                let eq = self.command(cursor) == Some("eqref");
                if let Some(group) = self.group_after(cursor) {
                    self.push_inline(
                        &mut output,
                        Inline::Reference {
                            target: self.text(group.inner_start, group.inner_end)?,
                            kind: if eq {
                                ReferenceKind::EqRef
                            } else {
                                ReferenceKind::Ref
                            },
                            span: self.span(cursor, group.after),
                        },
                    )?;
                    cursor = group.after;
                    continue;
                }
            }
            if self.command(cursor) == Some("url") {
                if let Some(group) = self.group_after(cursor) {
                    let target = self.text(group.inner_start, group.inner_end)?;
                    let span = self.span(cursor, group.after);
                    self.push_inline(
                        &mut output,
                        Inline::Link {
                            target: target.clone(),
                            label: vec![Inline::Text { text: target, span }],
                            span,
                        },
                    )?;
                    cursor = group.after;
                    continue;
                }
            }
            if self.command(cursor) == Some("href") {
                if let Some(url) = self.group_after(cursor) {
                    if let Some(label_group) = self.group_at(url.after) {
                        let span = self.span(cursor, label_group.after);
                        let target = self.text(url.inner_start, url.inner_end)?;
                        let label = self.inlines(
                            label_group.inner_start,
                            label_group.inner_end,
                            depth + 1,
                        )?;
                        self.push_inline(
                            &mut output,
                            Inline::Link {
                                target,
                                label,
                                span,
                            },
                        )?;
                        cursor = label_group.after;
                        continue;
                    }
                }
            }
            if self.math_pair(cursor, end) {
                let close = cursor + 2;
                self.push_inline(
                    &mut output,
                    Inline::Math {
                        source: self.text(cursor + 1, close - 1)?,
                        span: self.span(cursor, close),
                    },
                )?;
                cursor = close;
                continue;
            }
            let token = match self.tokens.get(cursor) {
                Some(token) => token,
                None => break,
            };
            if !matches!(token.kind, TokenKind::Comment) {
                self.push_text(&mut output, token.span.start, token.span.end)?;
            }
            cursor += 1;
        }
        Ok(output)
    }

    fn unsupported_command(&mut self, at: usize, name: &str) -> Result<usize, ParseError> {
        let after = self
            .group_after(at)
            .map_or(at.saturating_add(1), |group| group.after);
        let kind = if matches!(name, "input" | "include" | "includeonly") {
            WarningKind::IncludeDenied
        } else if is_dangerous(name) {
            WarningKind::UnsafeCommandIgnored
        } else {
            WarningKind::UnknownCommand
        };
        let reason = if kind == WarningKind::IncludeDenied {
            "include command was not followed".to_owned()
        } else if kind == WarningKind::UnsafeCommandIgnored {
            "unsafe TeX command was not executed".to_owned()
        } else {
            format!("unsupported command '\\{name}'")
        };
        let span = self.span(at, after);
        self.warning(kind, &reason, Some(span))?;
        self.push_block(Block::Unsupported {
            source: self.text(at, after)?,
            reason,
            span,
        })?;
        Ok(after)
    }

    fn malformed_unsupported(
        &mut self,
        at: usize,
        command: &str,
        reason: &str,
    ) -> Result<usize, ParseError> {
        let after = at.saturating_add(1);
        let span = self.span(at, after);
        self.warning(WarningKind::MalformedSyntax, reason, Some(span))?;
        self.push_block(Block::Unsupported {
            source: self.text(at, after)?,
            reason: format!("\\{command}: {reason}"),
            span,
        })?;
        Ok(after)
    }
    fn unsupported_span(
        &mut self,
        start: usize,
        end: usize,
        reason: &str,
    ) -> Result<usize, ParseError> {
        let span = self.span(start, end);
        self.push_block(Block::Unsupported {
            source: self.text(start, end)?,
            reason: reason.to_owned(),
            span,
        })?;
        Ok(end)
    }

    fn find_environment_end(&self, name: &str, start: usize, end: usize) -> Option<(usize, usize)> {
        let mut cursor = start;
        let mut nesting = 1usize;
        while cursor < end {
            let command = self.command(cursor);
            if matches!(command, Some("begin") | Some("end")) {
                let group = self.group_after(cursor)?;
                let candidate = self.text(group.inner_start, group.inner_end).ok()?;
                if candidate == name {
                    if command == Some("begin") {
                        nesting = nesting.checked_add(1)?;
                    } else {
                        nesting = nesting.checked_sub(1)?;
                        if nesting == 0 {
                            return Some((cursor, group.after));
                        }
                    }
                }
                cursor = group.after;
                continue;
            }
            cursor += 1;
        }
        None
    }
    fn group_after(&self, at: usize) -> Option<Group> {
        self.group_at(at.checked_add(1)?)
    }
    fn group_at(&self, at: usize) -> Option<Group> {
        if !matches!(self.tokens.get(at)?.kind, TokenKind::GroupOpen) {
            return None;
        }
        let mut cursor = at.checked_add(1)?;
        let inner_start = cursor;
        let mut depth = 1usize;
        while let Some(token) = self.tokens.get(cursor) {
            match token.kind {
                TokenKind::GroupOpen => depth = depth.checked_add(1)?,
                TokenKind::GroupClose => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(Group {
                            inner_start,
                            inner_end: cursor,
                            after: cursor.checked_add(1)?,
                        });
                    }
                }
                _ => {}
            }
            cursor = cursor.checked_add(1)?;
        }
        None
    }
    fn command(&self, at: usize) -> Option<&'a str> {
        let token = self.tokens.get(at)?;
        matches!(token.kind, TokenKind::ControlWord)
            .then(|| {
                self.source
                    .get(token.span.start.checked_add(1)?..token.span.end)
            })
            .flatten()
    }
    fn blank_at(&self, at: usize) -> bool {
        self.tokens
            .get(at)
            .filter(|token| matches!(token.kind, TokenKind::Whitespace))
            .and_then(|token| self.source.get(token.span.start..token.span.end))
            .is_some_and(|text| text.bytes().filter(|byte| *byte == b'\n').nth(1).is_some())
    }
    fn only_ignored(&self, start: usize, end: usize) -> bool {
        (start..end).all(|i| {
            self.tokens
                .get(i)
                .is_none_or(|t| matches!(t.kind, TokenKind::Whitespace | TokenKind::Comment))
        })
    }
    fn math_pair(&self, at: usize, end: usize) -> bool {
        at.checked_add(2).is_some_and(|close| {
            close < end
                && matches!(
                    self.tokens.get(at).map(|t| &t.kind),
                    Some(TokenKind::MathShift)
                )
                && matches!(
                    self.tokens.get(at + 1).map(|t| &t.kind),
                    Some(
                        TokenKind::Text
                            | TokenKind::ControlWord
                            | TokenKind::ControlSymbol(_)
                            | TokenKind::Whitespace
                    )
                )
                && matches!(
                    self.tokens.get(close).map(|t| &t.kind),
                    Some(TokenKind::MathShift)
                )
        })
    }
    fn text(&self, start: usize, end: usize) -> Result<String, ParseError> {
        let span = self.span(start, end);
        let value = span.slice(self.source).unwrap_or("");
        let mut text = String::new();
        text.try_reserve(value.len())
            .map_err(|_| ParseError::AllocationFailed {
                resource: "source text",
            })?;
        text.push_str(value);
        Ok(text)
    }
    fn span(&self, start: usize, end: usize) -> SourceSpan {
        let at = self.source.len();
        SourceSpan::new(
            self.tokens.get(start).map_or(at, |t| t.span.start),
            self.tokens
                .get(end.saturating_sub(1))
                .map_or(at, |t| t.span.end),
        )
    }
    fn push_text(
        &self,
        output: &mut Vec<Inline>,
        start: usize,
        end: usize,
    ) -> Result<(), ParseError> {
        let text = self.source.get(start..end).unwrap_or("");
        if text.is_empty() {
            return Ok(());
        }
        self.push_inline(
            output,
            Inline::Text {
                text: text.to_owned(),
                span: SourceSpan::new(start, end),
            },
        )
    }
    fn push_inline(&self, output: &mut Vec<Inline>, inline: Inline) -> Result<(), ParseError> {
        if output.len() >= self.limits.max_inlines_per_block {
            return Err(ParseError::LimitExceeded {
                resource: "inlines per block",
                maximum: self.limits.max_inlines_per_block,
            });
        }
        output
            .try_reserve(1)
            .map_err(|_| ParseError::AllocationFailed {
                resource: "inlines",
            })?;
        output.push(inline);
        Ok(())
    }
    fn push_block(&mut self, block: Block) -> Result<(), ParseError> {
        if self.document.blocks.len() >= self.limits.max_blocks {
            return Err(ParseError::LimitExceeded {
                resource: "blocks",
                maximum: self.limits.max_blocks,
            });
        }
        self.document
            .blocks
            .try_reserve(1)
            .map_err(|_| ParseError::AllocationFailed { resource: "blocks" })?;
        self.document.blocks.push(block);
        Ok(())
    }
    fn push_outline(&mut self, entry: OutlineEntry) -> Result<(), ParseError> {
        self.document
            .outline
            .try_reserve(1)
            .map_err(|_| ParseError::AllocationFailed {
                resource: "outline",
            })?;
        self.document.outline.push(entry);
        Ok(())
    }
    fn push_anchor(&mut self, anchor: Anchor) -> Result<(), ParseError> {
        self.document
            .anchors
            .try_reserve(1)
            .map_err(|_| ParseError::AllocationFailed {
                resource: "anchors",
            })?;
        self.document.anchors.push(anchor);
        Ok(())
    }
    fn warning(
        &mut self,
        kind: WarningKind,
        message: &str,
        span: Option<SourceSpan>,
    ) -> Result<(), ParseError> {
        let mut owned = String::new();
        owned
            .try_reserve(message.len())
            .map_err(|_| ParseError::AllocationFailed {
                resource: "warning message",
            })?;
        owned.push_str(message);
        self.push_warning(Warning {
            kind,
            message: owned,
            span,
        })
    }
    fn push_warning(&mut self, warning: Warning) -> Result<(), ParseError> {
        if self.document.warnings.len() >= self.limits.max_warnings {
            return Err(ParseError::LimitExceeded {
                resource: "warnings",
                maximum: self.limits.max_warnings,
            });
        }
        self.document
            .warnings
            .try_reserve(1)
            .map_err(|_| ParseError::AllocationFailed {
                resource: "warnings",
            })?;
        self.document.warnings.push(warning);
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Group {
    inner_start: usize,
    inner_end: usize,
    after: usize,
}

fn is_inline(name: &str) -> bool {
    matches!(
        name,
        "emph" | "textbf" | "texttt" | "href" | "url" | "ref" | "eqref"
    )
}
fn is_dangerous(name: &str) -> bool {
    matches!(
        name,
        "write" | "write18" | "openout" | "read" | "catcode" | "directlua"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_structure_links_formatting_and_references() {
        let doc = parse_tex_document("\\title{A}\\author{B}\n\\section{Intro}\\label{sec:i}\nText \\emph{x}, \\textbf{y}, \\texttt{z}; \\href{https://e}{E} \\url{https://u} \\eqref{eq:a}.").expect("parse");
        assert_eq!(doc.outline.len(), 1);
        assert_eq!(doc.outline[0].anchor.as_deref(), Some("sec:i"));
        assert_eq!(doc.anchors[0].name, "sec:i");
        assert!(doc.blocks.iter().any(|block| matches!(block, Block::Paragraph { content, .. } if content.iter().any(|inline| matches!(inline, Inline::Link { .. })) && content.iter().any(|inline| matches!(inline, Inline::Reference { kind: ReferenceKind::EqRef, .. })) )));
    }

    #[test]
    fn parses_safe_reader_environments() {
        let doc = parse_tex_document("\\begin{itemize}\\item first\\item second\\end{itemize}\\begin{quote}quoted\\end{quote}\\begin{verbatim}x\\end{verbatim}\\begin{equation}a=b\\end{equation}").expect("parse");
        assert!(
            matches!(doc.blocks[0], Block::List { ordered: false, ref items, .. } if items.len() == 2)
        );
        assert!(doc
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Quote { .. })));
        assert!(doc
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Code { source, .. } if source == "x")));
        assert!(doc
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Math { source, .. } if source == "a=b")));
    }

    #[test]
    fn includes_and_unknown_commands_are_visible_and_never_executed() {
        let doc = parse_tex_document("before \\input{secret.tex} \\mystery{value}").expect("parse");
        assert!(doc
            .warnings
            .iter()
            .any(|warning| warning.kind == WarningKind::IncludeDenied));
        assert!(doc
            .warnings
            .iter()
            .any(|warning| warning.kind == WarningKind::UnknownCommand));
        assert!(
            doc.blocks
                .iter()
                .filter(|block| matches!(block, Block::Unsupported { .. }))
                .count()
                >= 2
        );
    }

    #[test]
    fn enforces_parser_limits_without_panicking() {
        let parser = Parser::new(ParserLimits {
            max_blocks: 1,
            ..ParserLimits::default()
        });
        assert!(matches!(
            parser.parse("a\n\nb"),
            Err(ParseError::LimitExceeded {
                resource: "blocks",
                maximum: 1
            })
        ));
    }
}
