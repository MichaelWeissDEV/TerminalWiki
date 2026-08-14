//! `tw search QUERY` — search the knowledge base (spec §13).

use std::str::FromStr;

use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};
use terminalwiki_core::sanitize::sanitize_line;

use crate::args::Args;

pub fn search(query: String, args: Args, _config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let q = terminalwiki_index::Query::from_str(&query)
        .map_err(|_| Error::invalid_arguments("Invalid search query"))?;

    let mut any = false;
    let mut index_found = false;

    for wiki in wikis.iter() {
        // Load the index from disk. If missing, hint the user.
        let idx = match terminalwiki_index::WikiIndex::load(&wiki.name) {
            Ok(idx) => {
                index_found = true;
                idx
            }
            Err(_) => {
                // No index for this wiki yet.
                continue;
            }
        };

        if !idx.entries.is_empty() {
            index_found = true;
        }

        let results = idx.search(&q);
        for r in &results {
            any = true;
            let path = sanitize_line(&r.entry.relative.to_string_lossy());
            if args.path_only {
                println!("{}", path);
            } else if args.json {
                let title = sanitize_line(&r.entry.title);
                let wiki_name = sanitize_line(&wiki.name);
                println!(
                    "{{\"path\":\"{}\",\"title\":\"{}\",\"wiki\":\"{}\"}}",
                    path, title, wiki_name,
                );
            } else {
                let title = if r.entry.title.is_empty() {
                    path.clone()
                } else {
                    sanitize_line(&r.entry.title)
                };
                println!("{}", title);
                println!("  {}:{}", sanitize_line(&wiki.name), path);
                if let Some(ref snippet) = r.snippet {
                    println!("  {}", sanitize_line(snippet));
                }
                println!();
            }
        }

        if !args.all {
            break; // Only search default wiki unless --all
        }
    }

    if !any {
        if !index_found {
            eprintln!("No index found. Run 'tw index rebuild' first.");
        } else {
            eprintln!("No results for '{}'.", sanitize_line(&query));
        }
    }

    Ok(())
}