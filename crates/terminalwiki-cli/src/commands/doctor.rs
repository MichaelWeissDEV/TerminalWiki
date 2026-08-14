//! `tw doctor` — system diagnostics (spec §72).

use std::io::IsTerminal;

use terminalwiki_core::paths::{cache_dir, config_dir, config_file, state_dir};
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
        .map(|p| if p.exists() { "OK" } else { "not found" })
        .unwrap_or("unknown");
    println!("Configuration  {}", cfg_status);
    if let Some(p) = &cfg_path {
        println!("  {}", sanitize_line(&p.to_string_lossy()));
    }

    // Config dir
    if let Some(d) = config_dir() {
        println!("Config dir     {}", sanitize_line(&d.to_string_lossy()));
    }
    // Cache dir
    if let Some(d) = cache_dir() {
        println!("Cache dir      {}", sanitize_line(&d.to_string_lossy()));
    }
    // State dir
    if let Some(d) = state_dir() {
        println!("State dir      {}", sanitize_line(&d.to_string_lossy()));
    }

    println!();

    // Wikis
    if wikis.is_empty() {
        println!("Default wiki   (none configured)");
    } else {
        for wiki in wikis.iter() {
            let is_default = wikis.default_wiki().is_some_and(|d| d.name == wiki.name);
            let marker = if is_default { "* " } else { "  " };
            let exists = if wiki.root.exists() {
                ""
            } else {
                " (missing!)"
            };
            println!(
                "{}Wiki {}  {}{}",
                marker,
                sanitize_line(&wiki.name),
                sanitize_line(&wiki.root.to_string_lossy()),
                exists
            );

            // Index status
            let idx_status = if let Some(dir) = terminalwiki_core::paths::index_dir_for(&wiki.name)
            {
                if dir.join("entries.jsonl").exists() {
                    "built"
                } else {
                    "not built"
                }
            } else {
                "unknown"
            };
            println!("  Index        {}", idx_status);
        }
    }

    println!();

    // Terminal
    let term = std::env::var("TERM").unwrap_or_else(|_| "(not set)".to_string());
    let is_tty = std::io::stdout().is_terminal();
    let colorterm = std::env::var("COLORTERM").unwrap_or_else(|_| "(not set)".to_string());
    println!("Terminal       {}", sanitize_line(&term));
    println!("TTY            {}", if is_tty { "yes" } else { "no" });
    println!("COLORTERM      {}", sanitize_line(&colorterm));
    println!(
        "True color     {}",
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            "yes"
        } else {
            "unknown"
        }
    );
    println!(
        "NO_COLOR       {}",
        if std::env::var_os("NO_COLOR").is_some() {
            "set"
        } else {
            "not set"
        }
    );

    // tmux
    let tmux = std::env::var("TMUX").is_ok();
    println!("tmux           {}", if tmux { "yes" } else { "no" });

    println!();

    // Editor
    let editor = crate::editor::resolve_editor(&config);
    println!("Editor         {}", sanitize_line(&editor));

    // Rust version / build info
    println!(
        "Compiled with  rustc {}",
        env!("CARGO_PKG_RUST_VERSION", "unknown")
    );

    Ok(())
}
