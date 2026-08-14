//! `tw raw PAGE` — output raw unrendered markdown / content (spec §12).

use std::fs;
use std::io::{stdout, Write};

use terminalwiki_core::resolve;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::Args;

pub fn raw(page: String, args: Args, config: Config, wikis: WikiSet) -> Result<()> {
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

    let bytes = fs::read(&resolution.path).map_err(|e| Error::io(&resolution.path, e))?;
    stdout()
        .write_all(&bytes)
        .map_err(|e| Error::other(format!("Write error: {}", e)))?;

    Ok(())
}