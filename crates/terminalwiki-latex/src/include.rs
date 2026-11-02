//! Safe resolution of LaTeX `\input` and `\include` files.
//!
//! This module deliberately resolves paths only.  Opening a resolved file is
//! the caller's responsibility; it never writes to the filesystem.

use std::fmt;
use std::path::{Path, PathBuf};

use terminalwiki_core::wiki::resolve_within;

/// The largest permitted nesting level for LaTeX includes.
pub const MAX_INCLUDE_DEPTH: usize = 8;

/// Errors produced while resolving or tracking a LaTeX include.
#[derive(Debug)]
pub enum IncludeError {
    /// The supplied source or requested target is not a local TeX document.
    UnsupportedPath { path: PathBuf, reason: &'static str },
    /// The core wiki resolver refused a path which would leave the wiki root.
    OutsideWiki { path: PathBuf, reason: String },
    /// The requested include does not name a regular file.
    NotAFile { path: PathBuf },
    /// Include nesting exceeded [`MAX_INCLUDE_DEPTH`].
    DepthExceeded { max_depth: usize },
    /// An include would re-enter a file already being processed.
    Cycle { path: PathBuf },
}

impl fmt::Display for IncludeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPath { path, reason } => {
                write!(f, "refused LaTeX include {}: {reason}", path.display())
            }
            Self::OutsideWiki { path, reason } => {
                write!(f, "refused LaTeX include {}: {reason}", path.display())
            }
            Self::NotAFile { path } => write!(f, "LaTeX include is not a file: {}", path.display()),
            Self::DepthExceeded { max_depth } => {
                write!(f, "LaTeX include nesting exceeds {max_depth} files")
            }
            Self::Cycle { path } => write!(f, "LaTeX include cycle at {}", path.display()),
        }
    }
}

impl std::error::Error for IncludeError {}

/// Resolves a requested `\input` or `\include` relative to its source file.
///
/// `wiki_root` is the configured wiki root and `source_relative` must be a
/// wiki-relative `.tex` or `.latex` source path.  A requested path without an
/// extension follows TeX's conventional `.tex` fallback.  URLs, absolute
/// paths, non-TeX paths, directories, and targets outside the wiki are
/// refused.
pub fn resolve_include(
    wiki_root: &Path,
    source_relative: &Path,
    requested: &Path,
) -> Result<PathBuf, IncludeError> {
    ensure_local_tex_path(source_relative)?;
    // Resolve the source too: a source path using traversal or an escaping
    // symlink must not provide the base directory for a later include.
    resolve_within(wiki_root, source_relative).map_err(core_path_error)?;

    ensure_local_requested_path(requested)?;
    let requested = with_tex_extension(requested);
    ensure_tex_extension(&requested)?;

    let relative_target = source_relative
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(requested);
    let target = resolve_within(wiki_root, &relative_target).map_err(core_path_error)?;

    if !target.is_file() {
        return Err(IncludeError::NotAFile { path: target });
    }
    Ok(target)
}

/// Tracks the active include stack, enforcing depth and cycle limits.
///
/// Paths passed to [`IncludeStack::enter`] should be the canonical paths
/// returned by [`resolve_include`].  The stack has no filesystem side effects.
#[derive(Debug, Default)]
pub struct IncludeStack {
    active: Vec<PathBuf>,
}

impl IncludeStack {
    /// Creates an empty active include stack.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a file as active, rejecting cycles and excessive nesting.
    pub fn enter(&mut self, path: PathBuf) -> Result<(), IncludeError> {
        if self.active.contains(&path) {
            return Err(IncludeError::Cycle { path });
        }
        if self.active.len() >= MAX_INCLUDE_DEPTH {
            return Err(IncludeError::DepthExceeded {
                max_depth: MAX_INCLUDE_DEPTH,
            });
        }
        self.active.push(path);
        Ok(())
    }

    /// Removes the most recently entered file after it has been processed.
    pub fn leave(&mut self) -> Option<PathBuf> {
        self.active.pop()
    }

    /// Returns the files currently being processed, oldest first.
    pub fn active(&self) -> &[PathBuf] {
        &self.active
    }
}

fn core_path_error(error: terminalwiki_core::Error) -> IncludeError {
    match error {
        terminalwiki_core::Error::PathRefused { path, reason } => {
            IncludeError::OutsideWiki { path, reason }
        }
        other => IncludeError::OutsideWiki {
            path: PathBuf::new(),
            reason: other.to_string(),
        },
    }
}

fn ensure_local_tex_path(path: &Path) -> Result<(), IncludeError> {
    if path.is_absolute() {
        return Err(IncludeError::UnsupportedPath {
            path: path.to_path_buf(),
            reason: "absolute paths are not allowed",
        });
    }
    if looks_like_url(path) {
        return Err(IncludeError::UnsupportedPath {
            path: path.to_path_buf(),
            reason: "URLs are not allowed",
        });
    }
    ensure_tex_extension(path)
}

fn ensure_local_requested_path(path: &Path) -> Result<(), IncludeError> {
    if path.as_os_str().is_empty() {
        return Err(IncludeError::UnsupportedPath {
            path: path.to_path_buf(),
            reason: "an include path is required",
        });
    }
    if path.is_absolute() {
        return Err(IncludeError::UnsupportedPath {
            path: path.to_path_buf(),
            reason: "absolute paths are not allowed",
        });
    }
    if looks_like_url(path) {
        return Err(IncludeError::UnsupportedPath {
            path: path.to_path_buf(),
            reason: "URLs are not allowed",
        });
    }
    Ok(())
}

fn with_tex_extension(path: &Path) -> PathBuf {
    if path.extension().is_some() {
        path.to_path_buf()
    } else {
        path.with_extension("tex")
    }
}

fn ensure_tex_extension(path: &Path) -> Result<(), IncludeError> {
    let is_tex = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("tex") || extension.eq_ignore_ascii_case("latex")
        });
    if is_tex {
        Ok(())
    } else {
        Err(IncludeError::UnsupportedPath {
            path: path.to_path_buf(),
            reason: "only .tex and .latex files may be included",
        })
    }
}

fn looks_like_url(path: &Path) -> bool {
    let value = path.as_os_str().to_string_lossy();
    let Some(colon) = value.find(':') else {
        return false;
    };
    let scheme = &value[..colon];
    !scheme.is_empty()
        && scheme.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic() || (index > 0 && matches!(byte, b'+' | b'-' | b'.'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("terminalwiki-include-{unique}"));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn resolves_extensionless_include_below_source() {
        let root = temp_root();
        fs::create_dir_all(root.join("chapters")).unwrap();
        fs::write(root.join("chapters/main.tex"), "").unwrap();
        fs::write(root.join("chapters/part.tex"), "").unwrap();

        let got =
            resolve_include(&root, Path::new("chapters/main.tex"), Path::new("part")).unwrap();
        assert_eq!(
            got,
            fs::canonicalize(root.join("chapters/part.tex")).unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_traversal_and_absolute_paths() {
        let root = temp_root();
        fs::write(root.join("main.tex"), "").unwrap();

        assert!(matches!(
            resolve_include(&root, Path::new("main.tex"), Path::new("../../outside.tex")),
            Err(IncludeError::OutsideWiki { .. })
        ));
        assert!(matches!(
            resolve_include(&root, Path::new("main.tex"), Path::new("/etc/passwd.tex")),
            Err(IncludeError::UnsupportedPath { .. })
        ));
        assert!(matches!(
            resolve_include(
                &root,
                Path::new("main.tex"),
                Path::new("https://example.test/a.tex")
            ),
            Err(IncludeError::UnsupportedPath { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escaping_wiki() {
        use std::os::unix::fs::symlink;

        let root = temp_root();
        let outside = temp_root();
        fs::write(root.join("main.tex"), "").unwrap();
        fs::write(outside.join("secret.tex"), "").unwrap();
        symlink(&outside, root.join("escape")).unwrap();

        assert!(matches!(
            resolve_include(&root, Path::new("main.tex"), Path::new("escape/secret.tex")),
            Err(IncludeError::OutsideWiki { .. })
        ));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn detects_cycles_and_limits_depth() {
        let mut stack = IncludeStack::new();
        stack.enter(PathBuf::from("a.tex")).unwrap();
        assert!(matches!(
            stack.enter(PathBuf::from("a.tex")),
            Err(IncludeError::Cycle { .. })
        ));

        let mut stack = IncludeStack::new();
        for number in 0..MAX_INCLUDE_DEPTH {
            stack.enter(PathBuf::from(format!("{number}.tex"))).unwrap();
        }
        assert!(matches!(
            stack.enter(PathBuf::from("too-deep.tex")),
            Err(IncludeError::DepthExceeded { .. })
        ));
    }
}
