//! Platform-appropriate configuration, cache and state directories (spec §49).
//!
//! The spec shows the XDG paths literally (`~/.config`, `~/.cache`,
//! `~/.local/state`) while also demanding platform-appropriate directories and
//! forbidding hard-coded Linux paths. These two requirements are reconciled as
//! follows:
//!
//! * `XDG_CONFIG_HOME` / `XDG_CACHE_HOME` / `XDG_STATE_HOME` always win when
//!   set and absolute. This is what makes the paths configurable rather than
//!   hard-coded.
//! * On Unix (including macOS) we then fall back to the XDG defaults. This
//!   matches the spec's own examples and matches how terminal-native tools
//!   actually behave on macOS — `git`, `nvim` and `fish` all read `~/.config`
//!   there, and a terminal user editing their wiki config expects to find it
//!   next to those, not under `~/Library/Application Support`.
//! * On Windows we use `%APPDATA%` and `%LOCALAPPDATA%`, so the architecture
//!   is not Unix-only (spec §116).
//!
//! Every function is overridable through `TW_CONFIG_DIR`, `TW_CACHE_DIR` and
//! `TW_STATE_DIR`, which is what the test-suite uses to stay hermetic.

use std::path::{Path, PathBuf};

const APP: &str = "terminalwiki";

/// Reads an environment variable and accepts it only if it is an absolute path.
fn env_abs(key: &str) -> Option<PathBuf> {
    let raw = std::env::var_os(key)?;
    if raw.is_empty() {
        return None;
    }
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        Some(path)
    } else {
        None
    }
}

/// The user's home directory.
pub fn home_dir() -> Option<PathBuf> {
    if let Some(h) = env_abs("HOME") {
        return Some(h);
    }
    #[cfg(windows)]
    {
        if let Some(p) = env_abs("USERPROFILE") {
            return Some(p);
        }
    }
    None
}

/// Directory holding `config.toml` (spec §49).
pub fn config_dir() -> Option<PathBuf> {
    if let Some(p) = env_abs("TW_CONFIG_DIR") {
        return Some(p);
    }
    if let Some(p) = env_abs("XDG_CONFIG_HOME") {
        return Some(p.join(APP));
    }
    #[cfg(windows)]
    {
        if let Some(p) = env_abs("APPDATA") {
            return Some(p.join("TerminalWiki").join("config"));
        }
    }
    home_dir().map(|h| h.join(".config").join(APP))
}

/// Directory holding the search index, render and image caches (spec §49).
///
/// Everything under here must be safe to delete at any time.
pub fn cache_dir() -> Option<PathBuf> {
    if let Some(p) = env_abs("TW_CACHE_DIR") {
        return Some(p);
    }
    if let Some(p) = env_abs("XDG_CACHE_HOME") {
        return Some(p.join(APP));
    }
    #[cfg(windows)]
    {
        if let Some(p) = env_abs("LOCALAPPDATA") {
            return Some(p.join("TerminalWiki").join("cache"));
        }
    }
    home_dir().map(|h| h.join(".cache").join(APP))
}

/// Directory holding session state such as navigation history (spec §49).
pub fn state_dir() -> Option<PathBuf> {
    if let Some(p) = env_abs("TW_STATE_DIR") {
        return Some(p);
    }
    if let Some(p) = env_abs("XDG_STATE_HOME") {
        return Some(p.join(APP));
    }
    #[cfg(windows)]
    {
        if let Some(p) = env_abs("LOCALAPPDATA") {
            return Some(p.join("TerminalWiki").join("state"));
        }
    }
    home_dir().map(|h| h.join(".local").join("state").join(APP))
}

/// Path of the global configuration file.
pub fn config_file() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

/// Root of the on-disk index for a given wiki name.
pub fn index_dir_for(wiki: &str) -> Option<PathBuf> {
    cache_dir().map(|d| d.join("index").join(sanitize_component(wiki)))
}

/// Turns an arbitrary wiki name into one safe path component.
///
/// Wiki names come from user configuration, so they must never be able to
/// escape the cache directory via `..` or a separator.
pub fn sanitize_component(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    // `.` and `..` would still be path-relative; neutralise them.
    if out.is_empty() || out.chars().all(|c| c == '.') {
        out.insert(0, 'w');
    }
    out
}

/// Expands a leading `~` in user-supplied configuration paths.
pub fn expand_tilde(path: &Path) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    if s == "~" {
        return home_dir().unwrap_or_else(|| path.to_path_buf());
    }
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_sanitization_cannot_escape_the_cache_dir() {
        // Dots survive (wiki names like `my.wiki` are legitimate); what matters
        // is that separators do not, so the result is always ONE component.
        assert_eq!(sanitize_component("../../etc"), ".._.._etc");
        assert_eq!(sanitize_component("a/b"), "a_b");
        assert_eq!(sanitize_component(".."), "w..");
        assert_eq!(sanitize_component(""), "w");
        assert_eq!(sanitize_component("rust"), "rust");
        assert_eq!(sanitize_component("my wiki"), "my_wiki");

        for evil in ["../..", "/abs", "a\\b", "..", "."] {
            let c = sanitize_component(evil);
            let joined = Path::new("/cache").join(&c);
            assert_eq!(joined.components().count(), 3, "{evil} -> {c} escaped");
        }
    }

    #[test]
    fn relative_env_overrides_are_ignored() {
        // A relative XDG path is invalid per the spec and must not be trusted.
        std::env::set_var("TW_TEST_REL", "relative/path");
        assert!(env_abs("TW_TEST_REL").is_none());
        std::env::remove_var("TW_TEST_REL");
    }

    #[test]
    fn tilde_expansion_only_applies_at_the_start() {
        let home = PathBuf::from("/home/tester");
        std::env::set_var("HOME", &home);
        assert_eq!(expand_tilde(Path::new("~/Knowledge")), home.join("Knowledge"));
        assert_eq!(expand_tilde(Path::new("~")), home);
        // A tilde in the middle is a legitimate file name character.
        assert_eq!(expand_tilde(Path::new("/a/~/b")), PathBuf::from("/a/~/b"));
    }
}
