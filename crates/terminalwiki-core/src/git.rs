//! Explicit git operations for vault synchronization.
//!
//! Design rules:
//! - Git is invoked with **direct argv, never through a shell**, so branch
//!   names, paths and commit messages can never inject commands.
//! - Every operation runs only when the user explicitly asks for it
//!   (`tw vault …`). Nothing here runs automatically, on a timer, or as a side
//!   effect of another command.
//! - Git output that may contain attacker-controlled text (branch names,
//!   remote names, git's own messages) is sanitized before it is embedded in
//!   errors or reports.
//! - A vault is only ever synced when the wiki root *is* the repository root.
//!   If the wiki merely lives inside some larger repository, `git add -A`
//!   would stage unrelated files outside the vault, so that case is refused.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};
use crate::sanitize::sanitize_text;

/// Snapshot of `git status --porcelain=v2 --branch` for one vault.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VaultStatus {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub staged: usize,
    pub modified: usize,
    pub untracked: usize,
    pub unmerged: usize,
}

impl VaultStatus {
    /// True when there is nothing to commit and no divergence from upstream.
    pub fn is_clean(&self) -> bool {
        self.staged == 0
            && self.modified == 0
            && self.untracked == 0
            && self.unmerged == 0
            && self.ahead == 0
            && self.behind == 0
    }
}

/// Runs git with direct argv and captures its output.
fn git(root: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| Error::other(format!("git is not available: {error}")))
}

/// Runs git, turning a non-zero exit into an error carrying git's message.
fn git_ok(root: &Path, args: &[&str], context: &str) -> Result<()> {
    let output = git(root, args)?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = sanitize_text(&String::from_utf8_lossy(&output.stderr));
    let stderr = stderr.trim();
    Err(Error::other(if stderr.is_empty() {
        format!("{context} failed (git exited with {})", output.status)
    } else {
        format!("{context} failed: {stderr}")
    }))
}

/// The repository toplevel, but only when it is exactly `root`.
///
/// A wiki that merely lives *inside* some other repository must not be synced
/// with `git add -A`: that would stage unrelated files outside the vault.
fn own_repository_root(root: &Path) -> Option<PathBuf> {
    let output = git(root, &["rev-parse", "--show-toplevel"]).ok()?;
    if !output.status.success() {
        return None;
    }
    let top = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    let root_canonical = root.canonicalize().ok()?;
    let top_canonical = top.canonicalize().ok()?;
    (top_canonical == root_canonical).then_some(top)
}

/// True when `root` itself is the working tree of a git repository.
pub fn is_repository(root: &Path) -> bool {
    own_repository_root(root).is_some()
}

/// Creates a git repository at `root`.
///
/// Returns `"created"` when a repository was made and `"existing"` when one
/// was already there, so the caller can phrase its message accordingly.
pub fn init(root: &Path) -> Result<&'static str> {
    if is_repository(root) {
        return Ok("existing");
    }
    git_ok(root, &["init"], "initializing git repository")?;
    Ok("created")
}

/// Reads branch, upstream and change counts for the vault.
pub fn status(root: &Path) -> Result<VaultStatus> {
    let output = git(root, &["status", "--porcelain=v2", "--branch"])?;
    if !output.status.success() {
        let stderr = sanitize_text(&String::from_utf8_lossy(&output.stderr));
        return Err(Error::other(format!(
            "git status failed: {}",
            stderr.trim()
        )));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut result = VaultStatus::default();
    for line in text.lines() {
        if let Some(head) = line.strip_prefix("# branch.head ") {
            result.branch = Some(head.to_string());
        } else if let Some(upstream) = line.strip_prefix("# branch.upstream ") {
            result.upstream = Some(upstream.to_string());
        } else if let Some(counts) = line.strip_prefix("# branch.ab ") {
            for part in counts.split_whitespace() {
                if let Some(ahead) = part.strip_prefix('+') {
                    result.ahead = ahead.parse().unwrap_or(0);
                } else if let Some(behind) = part.strip_prefix('-') {
                    result.behind = behind.parse().unwrap_or(0);
                }
            }
        } else if line.starts_with("? ") {
            result.untracked += 1;
        } else if line.starts_with("u ") {
            result.unmerged += 1;
        } else if line.starts_with("1 ") || line.starts_with("2 ") {
            // `1 XY …` / `2 XY …`: X is the index state, Y the worktree state.
            let xy = &line[2..4];
            let mut chars = xy.chars();
            let index_state = chars.next();
            let worktree_state = chars.next();
            if index_state.is_some_and(|state| state != '.' && state != '?') {
                result.staged += 1;
            }
            if worktree_state.is_some_and(|state| state != '.' && state != '?') {
                result.modified += 1;
            }
        }
    }
    Ok(result)
}

/// One explicit sync: stage, commit, pull and push.
///
/// Never force-pushes, never deletes anything, and stops with a clear error
/// as soon as git reports a problem (e.g. a rebase conflict that the user has
/// to resolve by hand).
pub fn sync(root: &Path, message: Option<&str>) -> Result<Vec<String>> {
    if !is_repository(root) {
        return Err(Error::other(
            "not a git repository — run 'tw vault init' first",
        ));
    }
    let mut report = Vec::new();

    git_ok(root, &["add", "-A"], "staging changes")?;
    report.push("Staged all changes".to_string());

    let staged = git(root, &["diff", "--cached", "--quiet"])?;
    match staged.status.code() {
        Some(0) => report.push("Nothing to commit".to_string()),
        Some(1) => {
            let message = message.unwrap_or("tw vault sync");
            // The message is a single argv element — no shell, no quoting.
            git_ok(root, &["commit", "-m", message], "committing")?;
            report.push(format!("Committed: {message}"));
        }
        code => {
            return Err(Error::other(format!(
                "git diff --cached failed (exit {code:?})"
            )));
        }
    }

    let remotes = git(root, &["remote"])?;
    if !remotes.status.success() {
        return Err(Error::other("listing git remotes failed"));
    }
    let remote_list = String::from_utf8_lossy(&remotes.stdout);
    let remote = remote_list
        .lines()
        .map(str::trim)
        .find(|name| name == &"origin")
        .or_else(|| remote_list.lines().map(str::trim).next())
        .map(str::to_string);

    let Some(remote) = remote else {
        report.push("No remote configured; changes stay local".to_string());
        return Ok(report);
    };

    let current = status(root)?;
    if current.upstream.is_some() {
        git_ok(root, &["pull", "--rebase"], "pulling with rebase")?;
        report.push("Pulled with rebase".to_string());
    } else {
        report.push("No upstream set; skipping pull".to_string());
    }

    if current.upstream.is_some() {
        git_ok(root, &["push"], "pushing")?;
    } else {
        git_ok(
            root,
            &["push", "-u", &remote, "HEAD"],
            "pushing (setting upstream)",
        )?;
    }
    report.push(format!("Pushed to {}", sanitize_text(&remote)));

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WikiEntry;
    use crate::wiki::Wiki;
    use std::fs;

    fn wiki(tag: &str) -> (tempfile::TempDir, Wiki) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join(tag);
        fs::create_dir_all(&root).expect("create root");
        let wiki = Wiki::open(&WikiEntry {
            name: tag.to_string(),
            path: root,
            mounts: Vec::new(),
        })
        .expect("open wiki");
        (tmp, wiki)
    }

    fn git_log_subject(root: &Path) -> String {
        let output = git(root, &["log", "-1", "--pretty=%s"]).expect("git log");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    #[test]
    fn a_fresh_directory_is_not_a_repository() {
        let (_tmp, wiki) = wiki("norepo");
        assert!(!is_repository(&wiki.root));
        assert!(sync(&wiki.root, None).is_err());
    }

    #[test]
    fn init_creates_a_repository_and_is_idempotent() {
        let (_tmp, wiki) = wiki("init");
        assert_eq!(init(&wiki.root).expect("init"), "created");
        assert_eq!(init(&wiki.root).expect("init again"), "existing");
        assert!(is_repository(&wiki.root));
    }

    #[test]
    fn status_counts_staged_modified_and_untracked_files() {
        let (_tmp, wiki) = wiki("status");
        init(&wiki.root).expect("init");
        fs::write(wiki.root.join("A.md"), "# A\n").expect("write A");

        let before = status(&wiki.root).expect("status");
        assert_eq!(before.untracked, 1);
        assert!(
            before.branch.as_deref() == Some("main")
                || before.branch.as_deref() == Some("master")
        );

        git_ok(&wiki.root, &["add", "A.md"], "stage").expect("add");
        fs::write(wiki.root.join("A.md"), "# A\n\nedited\n").expect("edit A");

        let after = status(&wiki.root).expect("status");
        assert_eq!(after.staged, 1);
        assert_eq!(after.modified, 1);
    }

    #[test]
    fn sync_commits_locally_when_no_remote_is_configured() {
        let (_tmp, wiki) = wiki("sync");
        init(&wiki.root).expect("init");
        let _ = git_ok(&wiki.root, &["config", "user.name", "Test User"], "config name");
        let _ = git_ok(&wiki.root, &["config", "user.email", "test@example.com"], "config email");
        fs::write(wiki.root.join("Note.md"), "# Note\n").expect("write");

        let report = sync(&wiki.root, Some("first sync")).expect("sync");
        assert!(
            report.iter().any(|line| line.contains("No remote")),
            "expected a no-remote note: {report:?}"
        );
        assert_eq!(git_log_subject(&wiki.root), "first sync");

        // A second sync with nothing to do must not create an empty commit.
        let report = sync(&wiki.root, None).expect("sync again");
        assert!(
            report.iter().any(|line| line == "Nothing to commit"),
            "{report:?}"
        );
        assert_eq!(git_log_subject(&wiki.root), "first sync");
    }

    #[test]
    fn sync_refuses_a_vault_that_lives_inside_a_foreign_repository() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let outer = tmp.path().join("outer");
        fs::create_dir_all(&outer).expect("outer");
        let vault = outer.join("vault");
        fs::create_dir_all(&vault).expect("vault");
        fs::write(vault.join("P.md"), "# P\n").expect("page");

        // Turn the *outer* directory into a repository.
        let output = Command::new("git")
            .args(["init"])
            .current_dir(&outer)
            .output()
            .expect("git init outer");
        assert!(output.status.success());

        let wiki = Wiki::open(&WikiEntry {
            name: "nested".to_string(),
            path: vault,
            mounts: Vec::new(),
        })
        .expect("open wiki");

        assert!(!is_repository(&wiki.root));
        let error = sync(&wiki.root, None).expect_err("sync must refuse");
        assert!(
            error.to_string().contains("tw vault init"),
            "unexpected error: {error}"
        );
    }
}
