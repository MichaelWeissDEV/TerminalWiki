//! `tw search QUERY` — search the knowledge base using Tantivy full-text index (spec §13, §14, §30-§34).

use std::str::FromStr;

use serde::Serialize;
use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::Args;

#[derive(Serialize)]
struct SearchJsonOutput {
    query: String,
    results: Vec<SearchResultJsonItem>,
}

#[derive(Serialize)]
struct SearchResultJsonItem {
    wiki: String,
    path: String,
    title: String,
    score: f32,
    snippet: Option<String>,
}

pub fn search(query: String, args: Args, _config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    // Mutually exclusive flag validation (Phase 34)
    if args.json && args.jsonl {
        return Err(Error::invalid_arguments(
            "Flags --json and --jsonl are mutually exclusive.",
        ));
    }
    if args.path_only && (args.json || args.jsonl) {
        return Err(Error::invalid_arguments(
            "Flag --path-only cannot be combined with --json or --jsonl.",
        ));
    }

    let q = terminalwiki_index::Query::from_str(&query)
        .map_err(|e| Error::invalid_arguments(format!("Invalid search query: {e}")))?;

    let target_wikis = crate::commands::wiki_selection::resolve_wiki_selection(&args, &wikis)?;

    let mut json_results = Vec::new();
    let mut any_hits = false;

    for wiki in target_wikis {
        let idx = terminalwiki_index::WikiIndex::load(&wiki.name).map_err(|e| {
            Error::index(format!(
                "Search index for '{}' is unavailable: {e}. Run 'tw index rebuild'.",
                wiki.name
            ))
        })?;

        let hits = idx.search(&q)?;

        for hit in hits {
            any_hits = true;
            let path_str = hit.relative.to_string_lossy().to_string();
            let clean_title = if hit.title.is_empty() {
                path_str.clone()
            } else {
                hit.title.clone()
            };

            if args.path_only {
                println!("{}", sanitize_line(&path_str));
            } else if args.jsonl {
                let item = SearchResultJsonItem {
                    wiki: hit.wiki.clone(),
                    path: path_str,
                    title: clean_title,
                    score: hit.score,
                    snippet: hit.snippet,
                };
                if let Ok(json_str) = serde_json::to_string(&item) {
                    println!("{json_str}");
                }
            } else if args.json {
                json_results.push(SearchResultJsonItem {
                    wiki: hit.wiki.clone(),
                    path: path_str,
                    title: clean_title,
                    score: hit.score,
                    snippet: hit.snippet,
                });
            } else {
                println!("{}", sanitize_line(&clean_title));
                println!(
                    "  {}:{}",
                    sanitize_line(&hit.wiki),
                    sanitize_line(&path_str)
                );
                if let Some(ref snippet) = hit.snippet {
                    println!("  {}", sanitize_line(snippet));
                }
                println!();
            }
        }
    }

    if args.json {
        let output = SearchJsonOutput {
            query: query.clone(),
            results: json_results,
        };
        let json_str = serde_json::to_string_pretty(&output)
            .map_err(|e| Error::other(format!("JSON serialization error: {e}")))?;
        println!("{json_str}");
        return Ok(());
    }

    if !any_hits && !args.jsonl && !args.path_only {
        eprintln!("No results found for '{}'.", sanitize_line(&query));
    }

    Ok(())
}
