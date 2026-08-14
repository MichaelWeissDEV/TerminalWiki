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
        println!("{}", wiki.name);
        if let Some(dir) = dir {
            let meta_file = dir.join("meta.json");
            if meta_file.exists() {
                if let Ok(text) = std::fs::read_to_string(&meta_file) {
                    if let Ok(meta) = serde_json::from_str::<terminalwiki_index::IndexMeta>(&text) {
                        let formatted_ts = terminalwiki_core::scan::format_timestamp(meta.built_at);
                        println!("  documents      {}", meta.document_count);
                        println!("  schema         {}", meta.schema_version);
                        println!("  updated        {}", formatted_ts);
                        println!("  state          ready");
                        continue;
                    }
                }
            }
            println!("  state          not built");
        } else {
            println!("  state          cache unavailable");
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