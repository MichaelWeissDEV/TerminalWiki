//! A lossless-enough, execution-free intermediate representation for LaTeX
//! documents.
//!
//! This module intentionally contains only data.  In particular, an AST value
//! cannot open files, expand macros, load URLs, or invoke a TeX engine.  The
//! parser keeps unsupported input as text and records a [`Warning`], allowing a
//! reader to present all source without treating TeX as executable code.

/// A parsed LaTeX document.
///
/// `blocks` preserve the reading order. `outline` and `anchors` are indexes for
/// navigation; they deliberately duplicate a small amount of information from
/// the blocks so consumers do not have to re-walk the tree to build navigation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub outline: Vec<OutlineEntry>,
    pub anchors: Vec<Anchor>,
    pub warnings: Vec<Warning>,
}

impl Document {
    /// Creates an empty document.
    pub const fn new() -> Self {
        Self {
            blocks: Vec::new(),
            outline: Vec::new(),
            anchors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Returns whether the document has no visible blocks.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

/// A top-level or nested document unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading {
        /// LaTeX section level, starting at 1 for `\section`.
        level: u8,
        content: Vec<Inline>,
        /// Label attached to this heading, if any.
        anchor: Option<String>,
        span: SourceSpan,
    },
    Paragraph {
        content: Vec<Inline>,
        span: SourceSpan,
    },
    /// A display-math expression. Its source is preserved for the native math
    /// renderer rather than being interpreted by a TeX engine.
    Math { source: String, span: SourceSpan },
    List {
        ordered: bool,
        items: Vec<ListItem>,
        span: SourceSpan,
    },
    Quote {
        blocks: Vec<Block>,
        span: SourceSpan,
    },
    Code {
        /// An optional language supplied by a safe, syntactic parser.
        language: Option<String>,
        source: String,
        span: SourceSpan,
    },
    Image {
        /// The literal path argument. Consumers must apply their own root
        /// confinement before reading it.
        source: String,
        alt: Vec<Inline>,
        span: SourceSpan,
    },
    /// Input that is intentionally retained instead of being executed,
    /// discarded, or silently approximated.
    Unsupported {
        source: String,
        reason: String,
        span: SourceSpan,
    },
}

/// One list item. Items contain blocks so that nested lists and paragraphs do
/// not need a lossy flattened representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    pub blocks: Vec<Block>,
    pub span: SourceSpan,
}

/// Text-level content in a block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inline {
    Text {
        text: String,
        span: SourceSpan,
    },
    Bold {
        content: Vec<Inline>,
        span: SourceSpan,
    },
    Italic {
        content: Vec<Inline>,
        span: SourceSpan,
    },
    Code {
        source: String,
        span: SourceSpan,
    },
    Link {
        /// Literal link destination. It is data only; a reader must never
        /// automatically fetch or open it.
        target: String,
        label: Vec<Inline>,
        span: SourceSpan,
    },
    Reference {
        /// The LaTeX label named by `\ref` or `\eqref`.
        target: String,
        kind: ReferenceKind,
        span: SourceSpan,
    },
    Math {
        source: String,
        span: SourceSpan,
    },
}

/// The syntactic form of a cross-reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    Ref,
    EqRef,
    PageRef,
    Custom,
}

/// A navigable heading entry, in document order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineEntry {
    pub level: u8,
    pub title: Vec<Inline>,
    /// The corresponding label, where the heading has one.
    pub anchor: Option<String>,
    /// Index of the associated entry in [`Document::blocks`].
    pub block_index: usize,
    pub span: SourceSpan,
}

/// A named `\label` target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    pub name: String,
    /// The block that owns the label. `None` represents a label which does not
    /// naturally belong to a visible block (for example, one in metadata).
    pub block_index: Option<usize>,
    pub span: SourceSpan,
}

/// A non-fatal parser diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    pub kind: WarningKind,
    pub message: String,
    /// Absent for document-level diagnostics.
    pub span: Option<SourceSpan>,
}

/// Categories of recoverable LaTeX-reader diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WarningKind {
    UnknownCommand,
    UnsupportedEnvironment,
    MalformedSyntax,
    UnsafeCommandIgnored,
    IncludeDenied,
    LimitExceeded,
}

/// A half-open byte range in the original UTF-8 source (`start..end`).
///
/// Byte offsets make it possible to preserve and display arbitrary LaTeX input
/// without decoding it a second time. Use [`SourceSpan::slice`] rather than
/// indexing a string directly; it returns `None` for invalid UTF-8 boundaries
/// or an invalid range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub const fn empty(at: usize) -> Self {
        Self::new(at, at)
    }

    pub const fn len(self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Returns the covered source only when this is a valid UTF-8 substring.
    pub fn slice(self, source: &str) -> Option<&str> {
        (self.start <= self.end)
            .then(|| source.get(self.start..self.end))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_span_safely_slices_utf8() {
        let source = "\u{00e4}x";
        assert_eq!(SourceSpan::new(0, 2).slice(source), Some("\u{00e4}"));
        assert_eq!(SourceSpan::new(1, 2).slice(source), None);
        assert_eq!(SourceSpan::new(2, 1).slice(source), None);
        assert_eq!(SourceSpan::new(0, 9).slice(source), None);
    }

    #[test]
    fn document_models_navigation_and_recoverable_input() {
        let heading_span = SourceSpan::new(0, 16);
        let anchor_span = SourceSpan::new(16, 31);
        let document = Document {
            blocks: vec![Block::Heading {
                level: 1,
                content: vec![Inline::Text {
                    text: "Introduction".into(),
                    span: SourceSpan::new(9, 21),
                }],
                anchor: Some("sec:intro".into()),
                span: heading_span,
            }],
            outline: vec![OutlineEntry {
                level: 1,
                title: vec![Inline::Text {
                    text: "Introduction".into(),
                    span: SourceSpan::new(9, 21),
                }],
                anchor: Some("sec:intro".into()),
                block_index: 0,
                span: heading_span,
            }],
            anchors: vec![Anchor {
                name: "sec:intro".into(),
                block_index: Some(0),
                span: anchor_span,
            }],
            warnings: vec![Warning {
                kind: WarningKind::UnsafeCommandIgnored,
                message: "\\write18 was not executed".into(),
                span: Some(SourceSpan::new(32, 40)),
            }],
        };

        assert!(!document.is_empty());
        assert_eq!(document.outline[0].block_index, 0);
        assert_eq!(document.anchors[0].name, "sec:intro");
        assert!(matches!(
            document.warnings[0].kind,
            WarningKind::UnsafeCommandIgnored
        ));
    }

    #[test]
    fn nested_content_and_unhandled_source_are_representable() {
        let span = SourceSpan::new(0, 1);
        let block = Block::List {
            ordered: false,
            items: vec![ListItem {
                blocks: vec![Block::Unsupported {
                    source: "\\begin{tikzpicture}".into(),
                    reason: "TikZ execution is not supported".into(),
                    span,
                }],
                span,
            }],
            span,
        };

        assert!(matches!(block, Block::List { ordered: false, .. }));
    }
}
