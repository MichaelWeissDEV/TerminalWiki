//! `tw index status/update/rebuild` — manage the search index (spec §15, §64).

use std::time::Instant;

use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::{Args, IndexCommand};

pub fn run(cmd: IndexCommand, _args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    match cmd {
        IndexCommand::Status => status(config, wikis),
        IndexCommand::Update => update(config, wikis),
        IndexCommand::Rebuild => rebuild(config, wikis),
    }
}

fn status(_config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        println!("No wikis configured. Add one with: tw wiki add main ~/Knowledge --default");
        return Ok(());
    }
    for wiki in wikis.iter() {
        let dir = terminalwiki_core::paths::index_dir_for(&wiki.name);
        let (status, entry_count) = if let Some(dir) = &dir {
            let entries_file = dir.join("entries.jsonl");
            let meta_file = dir.join("meta.json");
            if entries_file.exists() && meta_file.exists() {
                let count = count_lines(&entries_file).unwrap_or(0);
                ("built", Some(count))
            } else {
                ("not built", None)
            }
        } else {
            ("cache dir unavailable", None)
        };

        if let Some(n) = entry_count {
            println!("  {} — {} ({} pages indexed)", wiki.name, status, n);
        } else {
            println!("  {} — {}", wiki.name, status);
        }
        if let Some(dir) = dir {
            let meta = dir.join("meta.json");
            if meta.exists() {
                if let Ok(text) = std::fs::read_to_string(&meta) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if let Some(ts) = v.get("built_at").and_then(|t| t.as_u64()) {
                            let formatted = terminalwiki_core::scan::format_timestamp(ts);
                            println!("    Last built: {}", formatted);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn update(config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }
    for wiki in wikis.iter() {
        let t = Instant::now();
        eprint!("Updating index for '{}' … ", wiki.name);
        let idx = terminalwiki_index::WikiIndex::update(wiki, &config)
            .map_err(|e| Error::index(e.to_string()))?;
        eprintln!("done ({} pages, {:.1}s)", idx.entries.len(), t.elapsed().as_secs_f32());
    }
    Ok(())
}

fn rebuild(config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }
    for wiki in wikis.iter() {
        let t = Instant::now();
        eprint!("Rebuilding index for '{}' … ", wiki.name);
        let idx = terminalwiki_index::WikiIndex::build(wiki, &config)
            .map_err(|e| Error::index(e.to_string()))?;
        eprintln!("done ({} pages, {:.1}s)", idx.entries.len(), t.elapsed().as_secs_f32());
    }
    Ok(())
}

fn count_lines(path: &std::path::Path) -> std::io::Result<usize> {
    use std::io::{BufRead, BufReader};
    let f = std::fs::File::open(path)?;
    Ok(BufReader::new(f).lines().count())
}