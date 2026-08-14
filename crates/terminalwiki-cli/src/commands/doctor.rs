//! `tw doctor` — system diagnostics and terminal capability verification (spec §72, §73).

use terminalwiki_core::caps::TerminalCapabilities;
use terminalwiki_core::paths::{config_file, index_dir_for};
use terminalwiki_core::sanitize::sanitize_line;
use terminalwiki_core::wiki::WikiSet;
use terminalwiki_core::{Config, Result};

use crate::args::Args;

pub fn doctor(_args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    println!("TerminalWiki {}\n", version);

    // Configuration file
    let cfg_path = config_file();
    let cfg_status = cfg_path
        .as_ref()
        .map(|p| if p.exists() { "OK" } else { "missing" })
        .unwrap_or("missing");
    println!("Configuration     {}", cfg_status);

    // Default wiki and document count
    let default_wiki = wikis.default_wiki();
    if let Some(w) = default_wiki {
        println!("Default wiki      {}", sanitize_line(&w.name));

        if let Some(dir) = index_dir_for(&w.name) {
            if let Ok(Some(state)) = terminalwiki_index::store::load_meta(&dir) {
                println!("Index             ready");
                println!("Documents         {}", state.document_count);
            } else {
                println!("Index             not built");
            }
        } else {
            println!("Index             cache unavailable");
        }
    } else {
        println!("Default wiki      (none configured)");
        println!("Index             not built");
    }

    println!();

    // Terminal Capabilities (spec §73)
    let caps = TerminalCapabilities::detect();
    println!("Terminal          {}", sanitize_line(&caps.term_name));
    println!("TTY               {}", if caps.tty { "yes" } else { "no" });
    println!(
        "Unicode           {}",
        if caps.unicode { "yes" } else { "no" }
    );
    println!(
        "True color        {}",
        if caps.truecolor { "yes" } else { "no" }
    );
    println!(
        "Hyperlinks        {}",
        if caps.hyperlinks { "yes" } else { "no" }
    );
    println!("Graphics          {}", caps.graphics.as_str());
    println!("tmux              {}", if caps.tmux { "yes" } else { "no" });
    println!("SSH               {}", if caps.ssh { "yes" } else { "no" });

    println!();

    // Editor
    let editor = crate::editor::resolve_editor(&config);
    println!("Editor            {}", sanitize_line(&editor));

    Ok(())
}
