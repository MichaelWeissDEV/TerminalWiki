//! `tw find QUERY` — fuzzy-find a page by title or path (spec §13).

use terminalwiki_core::fuzzy;
use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::Args;

pub fn find(query: String, args: Args, _config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let mut candidates: Vec<(String, String, String)> = Vec::new(); // (wiki, path, title)

    // Prefer the index for titles, fall back to file scanning.
    for wiki in wikis.iter() {
        if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
            for entry in &idx.entries {
                let path_str = entry.relative.to_string_lossy().into_owned();
                candidates.push((wiki.name.clone(), path_str, entry.title.clone()));
            }
        } else {
            // No index — scan page filenames only.
            let files = terminalwiki_core::scan::scan(wiki, &terminalwiki_core::config::IndexConfig::default());
            for f in files {
                let path_str = f.relative.to_string_lossy().into_owned();
                candidates.push((wiki.name.clone(), path_str, String::new()));
            }
        }
        if !args.all {
            break;
        }
    }

    // Score each candidate against the query.
    let mut scored: Vec<(i32, &(String, String, String))> = candidates
        .iter()
        .filter_map(|c| {
            let title_score = if c.2.is_empty() {
                None
            } else {
                fuzzy::score(&query, &c.2).map(|m| (m.score + 20, c))
            };
            let path_score = fuzzy::score(&query, &c.1).map(|m| (m.score, c));

            title_score.or(path_score)
        })
        .collect();

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.dedup_by_key(|s| s.1.1.clone());

    let limit = 20;
    for (_, (wiki, path, title)) in scored.into_iter().take(limit) {
        let display_title = if title.is_empty() { path.as_str() } else { title.as_str() };
        if args.path_only {
            println!("{}", sanitize_line(path));
        } else {
            println!("{}", sanitize_line(display_title));
            println!("  {}:{}", sanitize_line(wiki), sanitize_line(path));
        }
    }

    Ok(())
}