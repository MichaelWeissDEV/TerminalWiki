//! `tw new PAGE` — create a new page and open it in the editor (spec §46).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use terminalwiki_core::wiki::{Wiki, WikiSet};
use terminalwiki_core::{Config, Error, Result};

use crate::args::Args;
use crate::editor::open_editor;

pub fn new_page(page: String, args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let wiki = args
        .wiki
        .as_deref()
        .map(|name| wikis.require(name))
        .transpose()?
        .or_else(|| wikis.default_wiki())
        .ok_or(Error::NoWikiConfigured)?;

    let clean_title = page.trim();
    if clean_title.is_empty() {
        return Err(Error::invalid_arguments("Page name cannot be empty"));
    }

    let filename = if clean_title.ends_with(".md") {
        clean_title.to_string()
    } else {
        format!("{}.md", config.naming.apply(clean_title))
    };

    let target_path = new_page_target(wiki, Path::new(&filename))?;

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    let initial_content = format!(
        "---\ntitle: {}\ntags: []\n---\n\n# {}\n\n",
        clean_title, clean_title
    );

    // `create_new` closes the check-then-create race for an existing page.
    // Path containment is verified before and after parent creation because a
    // missing directory can otherwise become a symlink while it is prepared.
    let target_path = new_page_target(wiki, Path::new(&filename))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target_path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                Error::other(format!("Page already exists at {}", target_path.display()))
            } else {
                Error::io(&target_path, e)
            }
        })?;
    file.write_all(initial_content.as_bytes())
        .map_err(|e| Error::io(&target_path, e))?;
    println!("Created page at {}", target_path.display());

    open_editor(&target_path, &config)?;

    Ok(())
}

/// Resolves a not-yet-created page through the wiki's containment boundary.
///
/// In particular this validates the nearest existing parent, so a symlink in
/// a newly-created page path cannot redirect a write outside the wiki.
fn new_page_target(wiki: &Wiki, filename: &Path) -> Result<PathBuf> {
    wiki.resolve_within(filename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use terminalwiki_core::config::WikiEntry;

    #[test]
    #[cfg(unix)]
    fn new_page_target_refuses_a_nonexistent_file_below_an_escaping_symlink() {
        let base = std::env::temp_dir().join(format!("tw-new-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("wiki");
        let outside = base.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        let wiki = Wiki::open(&WikiEntry {
            name: "test".into(),
            path: root,
            mounts: vec![],
        })
        .unwrap();

        let result = new_page_target(&wiki, Path::new("escape/new.md"));
        assert!(matches!(result, Err(Error::PathRefused { .. })));
        assert!(!outside.join("new.md").exists());
        fs::remove_dir_all(&base).ok();
    }
}
