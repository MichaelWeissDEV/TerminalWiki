//! `tw delete PAGE [--force]` — delete a page (spec §47).

use std::fs;
use std::io::{stdin, stdout, Write};

use terminalwiki_core::resolve;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::Args;

pub fn delete(
    page: String,
    force: bool,
    args: Args,
    config: Config,
    wikis: WikiSet,
) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let start_wiki = args
        .wiki
        .or_else(|| wikis.default_wiki().map(|w| w.name.clone()))
        .ok_or_else(|| Error::NoWikiConfigured)?;

    let resolution = resolve::resolve(&wikis, &start_wiki, &page, &config.index)?;

    if !resolution.path.exists() {
        return Err(Error::other(format!(
            "File does not exist: {}",
            resolution.path.display()
        )));
    }

    if !force {
        print!("Delete {}? [y/N]: ", resolution.path.display());
        stdout().flush().map_err(|e| Error::other(e.to_string()))?;

        let mut input = String::new();
        stdin()
            .read_line(&mut input)
            .map_err(|e| Error::other(e.to_string()))?;

        let trimmed = input.trim().to_ascii_lowercase();
        if trimmed != "y" && trimmed != "yes" {
            println!("Aborted.");
            return Ok(());
        }
    }

    fs::remove_file(&resolution.path).map_err(|e| Error::io(&resolution.path, e))?;
    println!("Deleted {}", resolution.path.display());

    Ok(())
}