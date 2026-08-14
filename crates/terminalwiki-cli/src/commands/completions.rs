//! `tw completions SHELL` — generate shell completion scripts (spec §100).

use std::io::{stdout, Write};
use terminalwiki_core::{Error, Result};

const BASH_COMPLETION: &str = r#"# Bash completion for TerminalWiki (tw)
_tw_completions() {
    local cur prev subcmds
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    subcmds="search find raw render new edit delete backlinks links related graph tags tag files query wiki index lint doctor stats config completions tui"

    case "$prev" in
        tw|terminalwiki)
            COMPREPLY=( $(compgen -W "$subcmds --help --version --plain --json --all --wiki" -- "$cur") )
            return 0
            ;;
        wiki)
            COMPREPLY=( $(compgen -W "list add remove rename mount unmount default" -- "$cur") )
            return 0
            ;;
        index)
            COMPREPLY=( $(compgen -W "status update rebuild" -- "$cur") )
            return 0
            ;;
        completions)
            COMPREPLY=( $(compgen -W "bash zsh fish" -- "$cur") )
            return 0
            ;;
        *)
            ;;
    esac
}
complete -F _tw_completions tw terminalwiki
"#;

const ZSH_COMPLETION: &str = r#"#compdef tw terminalwiki

_tw() {
    local -a commands
    commands=(
        'search:Search full text'
        'find:Fuzzy find pages'
        'raw:Output unrendered markdown'
        'render:Render file or stdin'
        'new:Create a new page'
        'edit:Edit a page'
        'delete:Delete a page'
        'backlinks:Show incoming links'
        'links:Show outgoing links'
        'related:Show related pages'
        'graph:Show link graph'
        'tags:List tags'
        'tag:Filter by tag'
        'files:List files'
        'wiki:Manage wikis'
        'index:Manage search index'
        'lint:Lint wiki for broken links'
        'doctor:Check system diagnostics'
        'stats:Wiki statistics'
        'config:Show active configuration'
        'completions:Generate shell completions'
        'tui:Open interactive TUI'
    )

    _arguments \
        '--wiki=[Specify wiki]:wiki:' \
        '--all[Search all wikis]' \
        '--plain[Plain text output]' \
        '--json[JSON output]' \
        '--no-color[Disable colors]' \
        '--version[Show version]' \
        '--help[Show help]' \
        '1: :->command' \
        '*:: :->args'

    case $state in
        command)
            _describe -t commands 'tw command' commands
            ;;
    esac
}

_tw "$@"
"#;

const FISH_COMPLETION: &str = r#"# Fish completion for TerminalWiki (tw)
complete -c tw -f
complete -c terminalwiki -f

complete -c tw -n "__fish_use_subcommand" -a search -d "Search full text"
complete -c tw -n "__fish_use_subcommand" -a find -d "Fuzzy find pages"
complete -c tw -n "__fish_use_subcommand" -a raw -d "Output unrendered markdown"
complete -c tw -n "__fish_use_subcommand" -a render -d "Render file or stdin"
complete -c tw -n "__fish_use_subcommand" -a new -d "Create a new page"
complete -c tw -n "__fish_use_subcommand" -a edit -d "Edit a page"
complete -c tw -n "__fish_use_subcommand" -a delete -d "Delete a page"
complete -c tw -n "__fish_use_subcommand" -a backlinks -d "Show incoming links"
complete -c tw -n "__fish_use_subcommand" -a links -d "Show outgoing links"
complete -c tw -n "__fish_use_subcommand" -a related -d "Show related pages"
complete -c tw -n "__fish_use_subcommand" -a graph -d "Show link graph"
complete -c tw -n "__fish_use_subcommand" -a tags -d "List tags"
complete -c tw -n "__fish_use_subcommand" -a tag -d "Filter by tag"
complete -c tw -n "__fish_use_subcommand" -a files -d "List files"
complete -c tw -n "__fish_use_subcommand" -a wiki -d "Manage wikis"
complete -c tw -n "__fish_use_subcommand" -a index -d "Manage search index"
complete -c tw -n "__fish_use_subcommand" -a lint -d "Lint wiki for broken links"
complete -c tw -n "__fish_use_subcommand" -a doctor -d "System diagnostics"
complete -c tw -n "__fish_use_subcommand" -a stats -d "Wiki statistics"
complete -c tw -n "__fish_use_subcommand" -a config -d "Show active configuration"
complete -c tw -n "__fish_use_subcommand" -a completions -d "Generate shell completions"
complete -c tw -n "__fish_use_subcommand" -a tui -d "Open interactive TUI"
"#;

pub fn generate(shell: String) -> Result<()> {
    match shell.to_ascii_lowercase().as_str() {
        "bash" => {
            stdout()
                .write_all(BASH_COMPLETION.as_bytes())
                .map_err(|e| Error::other(format!("Write error: {e}")))?;
        }
        "zsh" => {
            stdout()
                .write_all(ZSH_COMPLETION.as_bytes())
                .map_err(|e| Error::other(format!("Write error: {e}")))?;
        }
        "fish" => {
            stdout()
                .write_all(FISH_COMPLETION.as_bytes())
                .map_err(|e| Error::other(format!("Write error: {e}")))?;
        }
        _ => {
            return Err(Error::invalid_arguments(format!(
                "Unsupported shell: '{}'. Supported: bash, zsh, fish",
                shell
            )));
        }
    }
    Ok(())
}
