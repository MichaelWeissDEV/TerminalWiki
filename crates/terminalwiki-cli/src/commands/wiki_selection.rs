//! Shared wiki selection resolution for CLI commands (spec §8, §9).

use terminalwiki_core::wiki::{Wiki, WikiSet};
use terminalwiki_core::{Error, Result};

use crate::args::Args;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WikiSelection {
    Default,
    Named(String),
    All,
}

/// Resolves target wikis from CLI arguments consistently across all subcommands.
pub fn resolve_wiki_selection<'a>(args: &Args, wikis: &'a WikiSet) -> Result<Vec<&'a Wiki>> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    if args.all {
        Ok(wikis.iter().collect())
    } else if let Some(ref name) = args.wiki {
        Ok(vec![wikis.require(name)?])
    } else {
        let default_wiki = wikis.default_wiki().ok_or(Error::NoWikiConfigured)?;
        Ok(vec![default_wiki])
    }
}
