//! `tw tags [WIKI]` and `tw tag TAG` — list and filter by tags (spec §14).

use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::Args;

/// `tw tags` — list all tags and their page counts.
pub fn tags(_wiki: Option<String>, _args: Args, _config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let mut all_tags: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    for wiki in wikis.iter() {
        if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
            for (tag, pages) in idx.tags() {
                let entry = all_tags.entry(tag).or_default();
                for p in pages {
                    entry.push(format!("{}:{}", wiki.name, p));
                }
            }
        }
    }

    if all_tags.is_empty() {
        eprintln!("No tags found. Run 'tw index rebuild' to build the index.");
        return Ok(());
    }

    for (tag, pages) in &all_tags {
        println!("{} ({})", sanitize_line(tag), pages.len());
    }

    Ok(())
}

/// `tw tag TAG` — list all pages with a given tag.
pub fn tag(tag_query: String, _args: Args, _config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let mut found = false;
    for wiki in wikis.iter() {
        if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
            for (tag, pages) in idx.tags() {
                if tag.eq_ignore_ascii_case(&tag_query) {
                    for p in pages {
                        println!("{}:{}", sanitize_line(&wiki.name), sanitize_line(&p));
                        found = true;
                    }
                }
            }
        }
    }

    if !found {
        eprintln!("No pages found with tag '{}'.", sanitize_line(&tag_query));
    }

    Ok(())
}
