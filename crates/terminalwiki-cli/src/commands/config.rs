//! `tw config` — display current resolved configuration (spec §82).

use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Result};

use crate::args::Args;

pub fn show(_args: Args, config: Config, _wikis: WikiSet) -> Result<()> {
    println!("# TerminalWiki active configuration");
    println!(
        "default_wiki = {:?}",
        config.default_wiki.as_deref().unwrap_or("")
    );
    println!("editor = {:?}", config.editor.as_deref().unwrap_or(""));
    println!("theme = {:?}", config.theme);
    println!("naming = {:?}", config.naming);
    println!();
    println!("[render]");
    println!("max_content_width = {}", config.render.max_content_width);
    println!("line_numbers = {}", config.render.line_numbers);
    println!("graphics = {:?}", config.render.graphics);
    println!("math = {}", config.render.math);
    println!();
    println!("[index]");
    println!("code = {}", config.index.code);
    println!("hidden = {}", config.index.hidden);
    println!("max_file_size = {}", config.index.max_file_size);
    if !config.index.ignore.is_empty() {
        println!("ignore = {:?}", config.index.ignore);
    }
    println!();
    println!("[wikis]");
    for w in &config.wikis {
        println!("[[wikis]]");
        println!("name = {:?}", w.name);
        println!("path = {:?}", w.path.display().to_string());
        if !w.mounts.is_empty() {
            println!("mounts = {:?}", w.mounts);
        }
    }
    Ok(())
}
