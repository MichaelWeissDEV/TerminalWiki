//! `tw stats` — wiki statistics (spec §69).

use terminalwiki_core::filetype::ContentType;
use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::scan::scan;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Error, Result};

use crate::args::Args;

pub fn stats(_args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    if wikis.is_empty() {
        return Err(Error::NoWikiConfigured);
    }

    for wiki in wikis.iter() {
        let files = scan(wiki, &config.index);

        let total = files.len();
        let markdown = files.iter().filter(|f| f.content_type == ContentType::Markdown).count();
        let text = files.iter().filter(|f| f.content_type == ContentType::Text).count();
        let code = files.iter().filter(|f| f.content_type == ContentType::Code).count();
        let images = files.iter().filter(|f| f.content_type == ContentType::Image).count();
        let binary = files.iter().filter(|f| f.content_type == ContentType::Binary).count();
        let total_bytes: u64 = files.iter().map(|f| f.size).sum();

        println!("Wiki: {}", sanitize_line(&wiki.name));
        println!("  Root:     {}", sanitize_line(&wiki.root.to_string_lossy()));
        println!("  Files:    {}", total);
        if markdown > 0 {
            println!("  Markdown: {}", markdown);
        }
        if text > 0 {
            println!("  Text:     {}", text);
        }
        if code > 0 {
            println!("  Code:     {}", code);
        }
        if images > 0 {
            println!("  Images:   {}", images);
        }
        if binary > 0 {
            println!("  Binary:   {}", binary);
        }
        println!(
            "  Size:     {}",
            terminalwiki_core::filetype::human_size(total_bytes)
        );

        // Index stats if available.
        if let Ok(idx) = terminalwiki_index::WikiIndex::load(&wiki.name) {
            let tag_count = idx.tags().len();
            println!("  Indexed:  {} pages", idx.entries.len());
            if tag_count > 0 {
                println!("  Tags:     {}", tag_count);
            }
        }
        println!();
    }

    Ok(())
}