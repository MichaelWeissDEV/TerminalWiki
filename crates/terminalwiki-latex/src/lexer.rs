//! A deliberately small, non-executing lexer for TeX source.
//!
//! This module only recognizes lexical structure.  In particular, it does not
//! expand macros, resolve catcodes, read files, or otherwise execute TeX.

use std::fmt;

/// A half-open byte range in the original UTF-8 source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    /// Returns the source covered by this span, if it belongs to `source`.
    pub fn slice(self, source: &str) -> Option<&str> {
        source.get(self.start..self.end)
    }
}

/// A lexical token. Textual token contents are represented by `span` rather
/// than copied into the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: SourceSpan,
}

/// The syntactic category of a [`Token`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// A backslash followed by one or more ASCII letters, such as `\section`.
    ControlWord,
    /// A backslash followed by one non-letter character, such as `\%`.
    ControlSymbol(char),
    Text,
    Whitespace,
    /// A percent sign through, but not including, the following line ending.
    Comment,
    GroupOpen,
    GroupClose,
    MathShift,
    Ampersand,
    /// The `\\` control sequence.
    LineBreak,
}

/// A broad token category used in limit errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenClass {
    ControlWord,
    ControlSymbol,
    Text,
    Whitespace,
    Comment,
    Punctuation,
    LineBreak,
}

impl fmt::Display for TokenClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::ControlWord => "control word",
            Self::ControlSymbol => "control symbol",
            Self::Text => "text",
            Self::Whitespace => "whitespace",
            Self::Comment => "comment",
            Self::Punctuation => "punctuation",
            Self::LineBreak => "line break",
        };
        formatter.write_str(name)
    }
}

/// Hard resource limits used while lexing untrusted source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LexerLimits {
    pub max_input_bytes: usize,
    pub max_tokens: usize,
    pub max_token_bytes: usize,
    pub max_warnings: usize,
}

impl Default for LexerLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 8 * 1024 * 1024,
            max_tokens: 1_000_000,
            max_token_bytes: 1024 * 1024,
            max_warnings: 1024,
        }
    }
}

/// A non-fatal condition observed while lexing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexWarning {
    /// NUL is retained as text, but is unusual in TeX input and often signals
    /// an accidental binary input.
    NulByte { span: SourceSpan },
}

/// The successful, bounded output of the lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexedDocument {
    pub tokens: Vec<Token>,
    pub warnings: Vec<LexWarning>,
}

/// A failure that prevents a complete lexical result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexError {
    InputTooLarge {
        actual: usize,
        maximum: usize,
    },
    TokenLimitExceeded {
        maximum: usize,
    },
    TokenTooLong {
        kind: TokenClass,
        start: usize,
        length: usize,
        maximum: usize,
    },
    WarningLimitExceeded {
        maximum: usize,
    },
    DanglingControlSequence {
        offset: usize,
    },
    AllocationFailed {
        resource: &'static str,
    },
}

impl fmt::Display for LexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge { actual, maximum } => {
                write!(
                    formatter,
                    "input is {actual} bytes, above the {maximum}-byte limit"
                )
            }
            Self::TokenLimitExceeded { maximum } => {
                write!(formatter, "token count exceeds the {maximum}-token limit")
            }
            Self::TokenTooLong {
                kind,
                start,
                length,
                maximum,
            } => write!(
                formatter,
                "{kind} at byte {start} is {length} bytes, above the {maximum}-byte limit"
            ),
            Self::WarningLimitExceeded { maximum } => {
                write!(
                    formatter,
                    "warning count exceeds the {maximum}-warning limit"
                )
            }
            Self::DanglingControlSequence { offset } => {
                write!(
                    formatter,
                    "control sequence at byte {offset} has no following character"
                )
            }
            Self::AllocationFailed { resource } => {
                write!(formatter, "could not allocate {resource}")
            }
        }
    }
}

impl std::error::Error for LexError {}

/// Lexes TeX source using [`LexerLimits::default`].
pub fn lex(source: &str) -> Result<LexedDocument, LexError> {
    Lexer::default().lex(source)
}

/// A reusable TeX lexer with explicit resource limits.
#[derive(Debug, Clone, Copy)]
pub struct Lexer {
    limits: LexerLimits,
}

impl Default for Lexer {
    fn default() -> Self {
        Self::new(LexerLimits::default())
    }
}

impl Lexer {
    pub const fn new(limits: LexerLimits) -> Self {
        Self { limits }
    }

    pub const fn limits(&self) -> LexerLimits {
        self.limits
    }

    pub fn lex(&self, source: &str) -> Result<LexedDocument, LexError> {
        if source.len() > self.limits.max_input_bytes {
            return Err(LexError::InputTooLarge {
                actual: source.len(),
                maximum: self.limits.max_input_bytes,
            });
        }

        let mut warnings = Vec::new();
        for (offset, byte) in source.bytes().enumerate() {
            if byte == 0 {
                push_warning(
                    &mut warnings,
                    LexWarning::NulByte {
                        span: SourceSpan {
                            start: offset,
                            end: offset + 1,
                        },
                    },
                    self.limits,
                )?;
            }
        }

        let bytes = source.as_bytes();
        let mut tokens = Vec::new();
        let mut cursor = 0;
        while cursor < bytes.len() {
            let start = cursor;
            let (kind, class) = match bytes[cursor] {
                b'\\' => {
                    cursor += 1;
                    if cursor == bytes.len() {
                        return Err(LexError::DanglingControlSequence { offset: start });
                    }
                    if bytes[cursor] == b'\\' {
                        cursor += 1;
                        (TokenKind::LineBreak, TokenClass::LineBreak)
                    } else if is_ascii_letter(bytes[cursor]) {
                        cursor += 1;
                        while cursor < bytes.len() && is_ascii_letter(bytes[cursor]) {
                            cursor += 1;
                        }
                        (TokenKind::ControlWord, TokenClass::ControlWord)
                    } else {
                        let symbol = source[cursor..]
                            .chars()
                            .next()
                            .ok_or(LexError::DanglingControlSequence { offset: start })?;
                        cursor += symbol.len_utf8();
                        (TokenKind::ControlSymbol(symbol), TokenClass::ControlSymbol)
                    }
                }
                b'%' => {
                    cursor += 1;
                    while cursor < bytes.len() && bytes[cursor] != b'\n' && bytes[cursor] != b'\r' {
                        cursor += 1;
                    }
                    (TokenKind::Comment, TokenClass::Comment)
                }
                byte if is_whitespace(byte) => {
                    cursor += 1;
                    while cursor < bytes.len() && is_whitespace(bytes[cursor]) {
                        cursor += 1;
                    }
                    (TokenKind::Whitespace, TokenClass::Whitespace)
                }
                b'{' => {
                    cursor += 1;
                    (TokenKind::GroupOpen, TokenClass::Punctuation)
                }
                b'}' => {
                    cursor += 1;
                    (TokenKind::GroupClose, TokenClass::Punctuation)
                }
                b'$' => {
                    cursor += 1;
                    (TokenKind::MathShift, TokenClass::Punctuation)
                }
                b'&' => {
                    cursor += 1;
                    (TokenKind::Ampersand, TokenClass::Punctuation)
                }
                _ => {
                    cursor += 1;
                    while cursor < bytes.len() && !starts_special_token(bytes[cursor]) {
                        cursor += 1;
                    }
                    (TokenKind::Text, TokenClass::Text)
                }
            };
            push_token(&mut tokens, kind, class, start, cursor, self.limits)?;
        }

        Ok(LexedDocument { tokens, warnings })
    }
}

fn push_warning(
    warnings: &mut Vec<LexWarning>,
    warning: LexWarning,
    limits: LexerLimits,
) -> Result<(), LexError> {
    if warnings.len() >= limits.max_warnings {
        return Err(LexError::WarningLimitExceeded {
            maximum: limits.max_warnings,
        });
    }
    warnings
        .try_reserve(1)
        .map_err(|_| LexError::AllocationFailed {
            resource: "warnings",
        })?;
    warnings.push(warning);
    Ok(())
}

fn push_token(
    tokens: &mut Vec<Token>,
    kind: TokenKind,
    class: TokenClass,
    start: usize,
    end: usize,
    limits: LexerLimits,
) -> Result<(), LexError> {
    let length = end - start;
    if length > limits.max_token_bytes {
        return Err(LexError::TokenTooLong {
            kind: class,
            start,
            length,
            maximum: limits.max_token_bytes,
        });
    }
    if tokens.len() >= limits.max_tokens {
        return Err(LexError::TokenLimitExceeded {
            maximum: limits.max_tokens,
        });
    }
    tokens
        .try_reserve(1)
        .map_err(|_| LexError::AllocationFailed { resource: "tokens" })?;
    tokens.push(Token {
        kind,
        span: SourceSpan { start, end },
    });
    Ok(())
}

const fn is_ascii_letter(byte: u8) -> bool {
    byte.is_ascii_uppercase() || byte.is_ascii_lowercase()
}

const fn is_whitespace(byte: u8) -> bool {
    byte == b' ' || (byte >= b'\t' && byte <= b'\r')
}

const fn starts_special_token(byte: u8) -> bool {
    byte == b'\\'
        || byte == b'%'
        || byte == b'{'
        || byte == b'}'
        || byte == b'$'
        || byte == b'&'
        || is_whitespace(byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_all_token_kinds_and_preserves_spans() {
        let source = "\\alpha \\%{x}$a&b\\\\% note\n";
        let result = lex(source).unwrap();
        let kinds: Vec<&TokenKind> = result.tokens.iter().map(|token| &token.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &TokenKind::ControlWord,
                &TokenKind::Whitespace,
                &TokenKind::ControlSymbol('%'),
                &TokenKind::GroupOpen,
                &TokenKind::Text,
                &TokenKind::GroupClose,
                &TokenKind::MathShift,
                &TokenKind::Text,
                &TokenKind::Ampersand,
                &TokenKind::Text,
                &TokenKind::LineBreak,
                &TokenKind::Comment,
                &TokenKind::Whitespace,
            ]
        );
        assert_eq!(result.tokens[0].span.slice(source), Some("\\alpha"));
        assert_eq!(result.tokens[10].span.slice(source), Some("\\\\"));
        assert_eq!(result.tokens[11].span.slice(source), Some("% note"));
    }

    #[test]
    fn spans_are_utf8_byte_offsets() {
        let source = "é\\ß";
        let result = lex(source).unwrap();
        assert_eq!(result.tokens[0].span, SourceSpan { start: 0, end: 2 });
        assert_eq!(result.tokens[1].span, SourceSpan { start: 2, end: 5 });
        assert_eq!(result.tokens[1].kind, TokenKind::ControlSymbol('ß'));
    }

    #[test]
    fn reports_bounded_resource_errors() {
        let lexer = Lexer::new(LexerLimits {
            max_input_bytes: 3,
            max_tokens: 1,
            max_token_bytes: 2,
            max_warnings: 0,
        });
        assert!(matches!(
            lexer.lex("four"),
            Err(LexError::InputTooLarge { .. })
        ));
        assert!(matches!(
            lexer.lex("abc"),
            Err(LexError::TokenTooLong { .. })
        ));
        assert!(matches!(
            lexer.lex("\0"),
            Err(LexError::WarningLimitExceeded { maximum: 0 })
        ));

        let token_limited = Lexer::new(LexerLimits {
            max_input_bytes: 8,
            max_tokens: 1,
            max_token_bytes: 8,
            max_warnings: 1,
        });
        assert!(matches!(
            token_limited.lex("a b"),
            Err(LexError::TokenLimitExceeded { maximum: 1 })
        ));
    }

    #[test]
    fn rejects_a_dangling_backslash_without_panicking() {
        assert_eq!(
            lex("text\\"),
            Err(LexError::DanglingControlSequence { offset: 4 })
        );
    }

    #[test]
    fn warns_for_nul_but_keeps_it_as_text() {
        let result = lex("a\0b").unwrap();
        assert_eq!(result.tokens.len(), 1);
        assert_eq!(result.tokens[0].span.slice("a\0b"), Some("a\0b"));
        assert_eq!(
            result.warnings,
            vec![LexWarning::NulByte {
                span: SourceSpan { start: 1, end: 2 }
            }]
        );
    }
}
