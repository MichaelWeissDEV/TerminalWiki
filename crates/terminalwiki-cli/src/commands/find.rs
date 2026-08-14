//! `tw find QUERY` — fast fuzzy page finder using Nucleo (spec §13, §14).

use serde::Serialize;
use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::Args;

#[derive(Serialize)]
struct FuzzyJsonOutput {
    query: String,
    results: Vec<FuzzyResultJsonItem>,
}

#[derive(Serialize)]
struct FuzzyResultJsonItem {
    wiki: String,
    path: String,
    title: String,
    score: u32,
}

pub fn find(query: String, args: Args, _config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    let mut json_results = Vec::new();
    let mut all_hits = Vec::new();

    for wiki in wikis.iter() {
        if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
            let hits = idx.find(&query, 20);
            for hit in hits {
                all_hits.push(hit);
            }
        }
        if !args.all {
            break;
        }
    }

    // Sort by score descending
    all_hits.sort_by(|a, b| b.score.cmp(&a.score));
    all_hits.truncate(20);

    if args.json {
        for hit in &all_hits {
            json_results.push(FuzzyResultJsonItem {
                wiki: hit.wiki.clone(),
                path: hit.relative.to_string_lossy().to_string(),
                title: hit.title.clone(),
                score: hit.score,
            });
        }
        let output = FuzzyJsonOutput {
            query: query.clone(),
            results: json_results,
        };
        if let Ok(json_str) = serde_json::to_string_pretty(&output) {
            println!("{json_str}");
        }
        return Ok(());
    }

    if all_hits.is_empty() {
        eprintln!("No matches found for '{}'.", sanitize_line(&query));
        return Ok(());
    }

    for hit in all_hits {
        let path_str = hit.relative.to_string_lossy().to_string();
        let display_title = if hit.title.is_empty() {
            path_str.as_str()
        } else {
            hit.title.as_str()
        };

        if args.path_only {
            println!("{}", sanitize_line(&path_str));
        } else {
            println!("{}", sanitize_line(display_title));
            println!("  {}:{}", sanitize_line(&hit.wiki), sanitize_line(&path_str));
        }
    }

    Ok(())
}