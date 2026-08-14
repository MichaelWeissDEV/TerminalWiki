pub mod args;
pub mod commands;
pub mod output;
pub mod editor;

use terminalwiki_core::ExitCode;
use terminalwiki_core::config;
use terminalwiki_core::wiki::WikiSet;

/// Main entry point for the TerminalWiki CLI.
pub fn main() -> std::process::ExitCode {
    let args_ext: Vec<String> = std::env::args().collect();
    let args = match args::Args::parse(&args_ext) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::InvalidArguments.into();
        }
    };

    if args.help {
        print_help();
        return ExitCode::Success.into();
    }
    if args.version {
        println!("terminalwiki 0.1.0");
        return ExitCode::Success.into();
    }

    let config = match config::load() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            return ExitCode::ConfigError.into();
        }
    };

    let (wikis, wiki_errors) = WikiSet::open(&config);
    for e in &wiki_errors {
        eprintln!("Warning: {}", e);
    }

    let result = commands::run(args, config, wikis);
    match result {
        Ok(_) => ExitCode::Success.into(),
        Err(e) => {
            eprintln!("{}", e);
            e.exit_code().into()
        }
    }
}

fn print_help() {
    println!("terminalwiki 0.1.0");
    println!("A fast, terminal-native knowledge base.\n");
    println!("USAGE");
    println!("  tw [OPTIONS] [WIKI] [PAGE]");
    println!("  tw <COMMAND> [ARGS]\n");
    println!("COMMANDS");
    println!("  tw PAGE            display a page");
    println!("  tw search QUERY    search the knowledge base");
    println!("  tw find QUERY      fuzzy-find a page");
    println!("  tw tui             open the interactive interface");
    println!("  tw new PAGE        create a new page");
    println!("  tw edit PAGE       edit a page in your editor");
    println!("  tw backlinks PAGE  show what links to a page");
    println!("  tw links PAGE      show what a page links to");
    println!("  tw related PAGE    show related pages");
    println!("  tw graph [PAGE]    show the link graph");
    println!("  tw tags            list all tags");
    println!("  tw wiki ...        manage wikis");
    println!("  tw index ...       manage the search index");
    println!("  tw lint            check for broken links");
    println!("  tw doctor          check system configuration");
    println!("  tw stats           show wiki statistics\n");
    println!("OPTIONS");
    println!("  --wiki NAME        use a specific wiki");
    println!("  --plain            plain text output");
    println!("  --json             JSON output");
    println!("  --no-color         disable colors");
    println!("  --version          show version");
    println!("  --help             show this help\n");
    println!("More: tw <COMMAND> --help");
}
