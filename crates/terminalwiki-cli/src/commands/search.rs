//! `tw search QUERY` — search the knowledge base using Tantivy full-text index (spec §13, §14).

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

    let q = terminalwiki_index::Query::from_str(&query)
        .map_err(|e| Error::invalid_arguments(format!("Invalid search query: {e}")))?;

    let mut json_results = Vec::new();
    let mut any_hits = false;
    let mut index_checked = false;

    for wiki in wikis.iter() {
        let idx = match terminalwiki_index::WikiIndex::load(&wiki.name) {
            Ok(idx) => {
                index_checked = true;
                idx
            }
            Err(_) => {
                continue;
            }
        };

        if let Ok(hits) = idx.search(&q) {
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
                    println!("  {}:{}", sanitize_line(&hit.wiki), sanitize_line(&path_str));
                    if let Some(ref snippet) = hit.snippet {
                        println!("  {}", sanitize_line(snippet));
                    }
                    println!();
                }
            }
        }

        if !args.all {
            break; // Default to searching default wiki unless --all
        }
    }

    if args.json {
        let output = SearchJsonOutput {
            query: query.clone(),
            results: json_results,
        };
        if let Ok(json_str) = serde_json::to_string_pretty(&output) {
            println!("{json_str}");
        }
        return Ok(());
    }

    if !any_hits {
        if !index_checked {
            eprintln!("No index found. Run 'tw index rebuild' first.");
        } else {
            eprintln!("No results found for '{}'.", sanitize_line(&query));
        }
    }

    Ok(())
}