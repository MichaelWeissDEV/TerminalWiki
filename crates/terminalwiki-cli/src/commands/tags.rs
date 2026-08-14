//! `tw tags [WIKI]` and `tw tag TAG` — list and filter by tags (spec §14).

use std::collections::BTreeMap;

use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Result};

use crate::args::Args;
use crate::commands::wiki_selection::resolve_wiki_selection;

/// `tw tags` — list all tags and their page counts.
pub fn tags(
    wiki_opt: Option<String>,
    mut args: Args,
    _config: Config,
    wikis: WikiSet,
) -> Result<()> {
    if wiki_opt.is_some() {
        args.wiki = wiki_opt;
    }
    let target_wikis = resolve_wiki_selection(&args, &wikis)?;

    let mut all_tags: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for wiki in target_wikis {
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
pub fn tag(tag_query: String, args: Args, _config: Config, wikis: WikiSet) -> Result<()> {
    let target_wikis = resolve_wiki_selection(&args, &wikis)?;

    let mut found = false;
    for wiki in target_wikis {
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
