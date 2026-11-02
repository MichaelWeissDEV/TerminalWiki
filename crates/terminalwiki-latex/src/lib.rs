//! Native, execution-free LaTeX reader pipeline.
//!
//! This crate parses a deliberately bounded reader-compatible subset. It never
//! runs TeX, shell escapes, plugins, network requests, or writes source files.

pub mod ast;
pub mod include;
pub mod lexer;
pub mod parser;
pub mod render;

pub use ast::{Document, Warning};
pub use parser::{parse_tex_document, ParseError, Parser, ParserLimits};
pub use render::adapt_document;
