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
            if let Ok(Some(state)) = terminalwiki_index::store::load_meta(&dir) {
                let formatted_ts = terminalwiki_core::scan::format_timestamp(state.built_at);
                println!("  documents      {}", state.document_count);
                println!("  schema         {}", state.schema_version);
                println!("  updated        {}", formatted_ts);
                println!("  state          ready");
                continue;
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
        eprintln!(
            "done ({} pages, {:.1}s)",
            idx.entries.len(),
            t.elapsed().as_secs_f32()
        );
        report_skipped(&idx);
    }
    Ok(())
}

/// Reports files that could not be indexed. A file that vanished mid-scan is
/// benign and only worth a count; an unreadable file means its indexed content
/// is now stale, so each one is named.
fn report_skipped(idx: &terminalwiki_index::WikiIndex) {
    use terminalwiki_core::sanitize::sanitize_line;
    use terminalwiki_index::SkipReason;

    let mut vanished = 0usize;
    for skip in &idx.skipped {
        match &skip.reason {
            SkipReason::Vanished => vanished += 1,
            SkipReason::Unreadable(msg) => eprintln!(
                "  warning: could not read {}: {} (keeping previously indexed content)",
                sanitize_line(&skip.relative.to_string_lossy()),
                sanitize_line(msg)
            ),
        }
    }
    if vanished > 0 {
        eprintln!("  note: {vanished} file(s) removed during scan");
    }
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
        eprintln!(
            "done ({} pages, {:.1}s)",
            idx.entries.len(),
            t.elapsed().as_secs_f32()
        );
    }
    Ok(())
}
