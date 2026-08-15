pub mod completions;
pub mod config;
pub mod delete;
pub mod doctor;
pub mod edit;
pub mod files;
pub mod find;
pub mod graph;
pub mod index;
pub mod links;
pub mod lint;
pub mod new;
pub mod page;
pub mod raw;
pub mod render;
pub mod search;
pub mod stats;
pub mod tags;
pub mod wiki;
pub mod wiki_selection;

use crate::args::{Args, Command};
use terminalwiki_core::wiki::{Wiki, WikiSet};
use terminalwiki_core::{Config, Result};

/// Loads a wiki's index after reconciling it with the filesystem.
///
/// This is what makes the wiki directory the source of truth in practice: a
/// file created with any external tool is visible to the next command without a
/// manual `tw index update`.
///
/// All notices go to **stderr**, so `tw search --json | jq` stays parseable.
pub fn open_index(wiki: &Wiki, config: &Config) -> Result<terminalwiki_index::WikiIndex> {
    let (idx, reconciliation) =
        terminalwiki_index::WikiIndex::open_with(wiki, config, |msg| eprintln!("{msg}"))?;
    if let Some(msg) = reconciliation.user_message() {
        eprintln!("{msg}");
    }
    for skip in &idx.skipped {
        if let terminalwiki_index::SkipReason::Unreadable(reason) = &skip.reason {
            eprintln!(
                "warning: could not read {}: {}",
                terminalwiki_core::sanitize::sanitize_line(&skip.relative.to_string_lossy()),
                terminalwiki_core::sanitize::sanitize_line(reason)
            );
        }
    }
    Ok(idx)
}

pub fn run(mut args: Args, config: Config, wikis: WikiSet) -> Result<()> {
    let command = std::mem::take(&mut args.command);
    match command {
        Command::Home => page::show_home(args, config, wikis),
        Command::Page { wiki, page } => page::show_page(wiki, page, args, config, wikis),
        Command::Tui { wiki, page } => terminalwiki_tui::run_tui(&wikis, &config, wiki, page),
        Command::Search { query } => search::search(query, args, config, wikis),
        Command::Find { query } => find::find(query, args, config, wikis),
        Command::Raw { page } => raw::raw(page, args, config, wikis),
        Command::Render { file } => render::render(file, args, config, wikis),
        Command::New { page } => new::new_page(page, args, config, wikis),
        Command::Edit { page } => edit::edit(page, args, config, wikis),
        Command::Delete { page, force } => delete::delete(page, force, args, config, wikis),
        Command::Backlinks { page } => links::backlinks(page, args, config, wikis),
        Command::Links { page } => links::links(page, args, config, wikis),
        Command::Related { page } => links::related(page, args, config, wikis),
        Command::Graph {
            page,
            depth,
            format,
        } => graph::graph(page, depth, format, args, config, wikis),
        Command::Tags { wiki } => tags::tags(wiki, args, config, wikis),
        Command::Tag { tag } => tags::tag(tag, args, config, wikis),
        Command::Files { type_filter } => files::files(type_filter, args, config, wikis),
        Command::Query { query } => search::search(query, args, config, wikis),
        Command::Wiki(cmd) => wiki::run(cmd, args, config, wikis),
        Command::Index(cmd) => index::run(cmd, args, config, wikis),
        Command::Lint => lint::lint(args, config, wikis),
        Command::Doctor => doctor::doctor(args, config, wikis),
        Command::Stats => stats::stats(args, config, wikis),
        Command::Config => config::show(args, config, wikis),
        Command::Completions { shell } => completions::generate(shell),
    }
}
