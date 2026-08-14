//! `tw edit PAGE` — open a page in the user's editor (spec §45).

use terminalwiki_core::resolve;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::Args;
use crate::editor::open_editor;

pub fn edit(page: String, args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let start_wiki = args
        .wiki
        .or_else(|| wikis.default_wiki().map(|w| w.name.clone()))
        .ok_or(Error::NoWikiConfigured)?;

    let resolution = resolve::resolve(&wikis, &start_wiki, &page, &config.index)?;

    if !resolution.path.exists() {
        return Err(Error::other(format!(
            "File does not exist: {}",
            resolution.path.display()
        )));
    }

    open_editor(&resolution.path, &config)?;

    Ok(())
}
