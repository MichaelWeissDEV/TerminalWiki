//! Core data model for TerminalWiki.
//!
//! The central design rule of the whole project (spec, preamble): **the files
//! are the truth**. Everything in this crate is a view over ordinary files on
//! disk. No proprietary database is required to read or edit a knowledge base,
//! and deleting every cache TerminalWiki owns must lose exactly nothing.

pub mod caps;
pub mod config;
pub mod error;
pub mod filetype;
pub mod frontmatter;
pub mod fuzzy;
pub mod link;
pub mod paths;
pub mod resolve;
pub mod sanitize;
pub mod scan;
pub mod unicode;
pub mod watch;
pub mod wiki;

pub use config::{Config, Theme};
pub use error::{Error, ExitCode, Result};
pub use filetype::ContentType;
pub use frontmatter::Frontmatter;
pub use link::{LinkTarget, WikiLink};
pub use unicode::{display_width, pad_display_width, truncate_display_width};
pub use wiki::{Wiki, WikiSet};
