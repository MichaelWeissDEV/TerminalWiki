//! `tw new PAGE` — create a new page and open it in the editor (spec §46).

use std::fs;

use terminalwiki_core::wiki::WikiSet;
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

    let target_path = wiki.root.join(&filename);

    if target_path.exists() {
        return Err(Error::other(format!(
            "Page already exists at {}",
            target_path.display()
        )));
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }

    let initial_content = format!(
        "---\ntitle: {}\ntags: []\n---\n\n# {}\n\n",
        clean_title, clean_title
    );

    fs::write(&target_path, initial_content).map_err(|e| Error::io(&target_path, e))?;
    println!("Created page at {}", target_path.display());

    open_editor(&target_path, &config)?;

    Ok(())
}