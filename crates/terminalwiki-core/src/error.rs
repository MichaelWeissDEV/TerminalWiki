//! Errors and the stable exit-code mapping (spec §93).
//!
//! Exit codes are part of the public contract and must not drift between
//! releases:
//!
//! ```text
//! 0 success
//! 1 general error
//! 2 invalid arguments
//! 3 page not found
//! 4 wiki not found
//! 5 index error
//! 6 configuration error
//! ```

use std::fmt;
use std::path::PathBuf;

/// Stable process exit codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    Success = 0,
    General = 1,
    InvalidArguments = 2,
    PageNotFound = 3,
    WikiNotFound = 4,
    IndexError = 5,
    ConfigError = 6,
}

impl ExitCode {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl From<ExitCode> for std::process::ExitCode {
    fn from(code: ExitCode) -> Self {
        std::process::ExitCode::from(code.as_u8())
    }
}

/// The error type shared by every TerminalWiki crate.
///
/// Errors carry enough structure for the CLI to render a compact, helpful
/// message (spec §73) without a Rust backtrace leaking into normal output.
#[derive(Debug)]
pub enum Error {
    /// A page could not be resolved. Carries fuzzy suggestions for "did you mean".
    PageNotFound {
        query: String,
        wiki: Option<String>,
        suggestions: Vec<String>,
    },
    /// A wiki name is not registered.
    WikiNotFound { name: String, known: Vec<String> },
    /// No wiki is configured at all.
    NoWikiConfigured,
    /// The configuration file is invalid or unreadable.
    Config {
        message: String,
        path: Option<PathBuf>,
    },
    /// An index could not be read, written or rebuilt.
    Index { message: String },
    /// Invalid arguments supplied by the user.
    InvalidArguments { message: String },
    /// A path escaped the wiki root or is otherwise refused (spec §42).
    PathRefused { path: PathBuf, reason: String },
    /// An I/O failure, annotated with the path it happened on.
    Io {
        path: Option<PathBuf>,
        source: std::io::Error,
    },
    /// Anything else that is still a clean, non-panicking failure.
    Other { message: String },
}

impl Error {
    /// The process exit code this error maps to.
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Error::PageNotFound { .. } => ExitCode::PageNotFound,
            Error::WikiNotFound { .. } | Error::NoWikiConfigured => ExitCode::WikiNotFound,
            Error::Config { .. } => ExitCode::ConfigError,
            Error::Index { .. } => ExitCode::IndexError,
            Error::InvalidArguments { .. } => ExitCode::InvalidArguments,
            Error::PathRefused { .. } | Error::Io { .. } | Error::Other { .. } => ExitCode::General,
        }
    }

    pub fn other(message: impl Into<String>) -> Self {
        Error::Other {
            message: message.into(),
        }
    }

    pub fn config(message: impl Into<String>) -> Self {
        Error::Config {
            message: message.into(),
            path: None,
        }
    }

    pub fn index(message: impl Into<String>) -> Self {
        Error::Index {
            message: message.into(),
        }
    }

    pub fn invalid_arguments(message: impl Into<String>) -> Self {
        Error::InvalidArguments {
            message: message.into(),
        }
    }

    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: Some(path.into()),
            source,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::PageNotFound { query, wiki, .. } => match wiki {
                Some(w) => write!(f, "Page not found in wiki '{w}': {query}"),
                None => write!(f, "Page not found: {query}"),
            },
            Error::WikiNotFound { name, .. } => write!(f, "Wiki not found: {name}"),
            Error::NoWikiConfigured => write!(
                f,
                "No wiki configured. Register one with:\n\n    tw wiki add main <path> --default"
            ),
            Error::Config { message, path } => match path {
                Some(p) => write!(f, "Configuration error in {}: {message}", p.display()),
                None => write!(f, "Configuration error: {message}"),
            },
            Error::Index { message } => write!(f, "Index error: {message}"),
            Error::InvalidArguments { message } => write!(f, "{message}"),
            Error::PathRefused { path, reason } => {
                write!(f, "Refused path {}: {reason}", path.display())
            }
            Error::Io { path, source } => match path {
                Some(p) => write!(f, "{}: {source}", p.display()),
                None => write!(f, "{source}"),
            },
            Error::Other { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(source: std::io::Error) -> Self {
        Error::Io { path: None, source }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_match_the_documented_contract() {
        assert_eq!(ExitCode::Success.as_u8(), 0);
        assert_eq!(ExitCode::General.as_u8(), 1);
        assert_eq!(ExitCode::InvalidArguments.as_u8(), 2);
        assert_eq!(ExitCode::PageNotFound.as_u8(), 3);
        assert_eq!(ExitCode::WikiNotFound.as_u8(), 4);
        assert_eq!(ExitCode::IndexError.as_u8(), 5);
        assert_eq!(ExitCode::ConfigError.as_u8(), 6);
    }

    #[test]
    fn errors_map_to_their_exit_codes() {
        let e = Error::PageNotFound {
            query: "heep".into(),
            wiki: None,
            suggestions: vec!["Heap".into()],
        };
        assert_eq!(e.exit_code(), ExitCode::PageNotFound);
        assert_eq!(Error::config("bad").exit_code(), ExitCode::ConfigError);
        assert_eq!(Error::index("bad").exit_code(), ExitCode::IndexError);
        assert_eq!(
            Error::invalid_arguments("bad").exit_code(),
            ExitCode::InvalidArguments
        );
        assert_eq!(
            Error::WikiNotFound {
                name: "x".into(),
                known: vec![]
            }
            .exit_code(),
            ExitCode::WikiNotFound
        );
    }
}
