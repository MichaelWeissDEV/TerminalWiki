//! Terminal capability detection and probe layer (spec §70-§74).

use std::env;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphicsProtocol {
    Kitty,
    Iterm2,
    Sixel,
    Unicode,
    Off,
}

impl GraphicsProtocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            GraphicsProtocol::Kitty => "kitty",
            GraphicsProtocol::Iterm2 => "iterm2",
            GraphicsProtocol::Sixel => "sixel",
            GraphicsProtocol::Unicode => "unicode",
            GraphicsProtocol::Off => "off",
        }
    }
}

/// Discovered terminal capabilities and feature support.
#[derive(Debug, Clone)]
pub struct TerminalCapabilities {
    pub tty: bool,
    pub term_name: String,
    pub unicode: bool,
    pub truecolor: bool,
    pub hyperlinks: bool,
    pub graphics: GraphicsProtocol,
    pub tmux: bool,
    pub ssh: bool,
}

impl TerminalCapabilities {
    /// Probes terminal environment variables and capabilities without hanging.
    pub fn detect() -> Self {
        let term = env::var("TERM").unwrap_or_default();
        let colorterm = env::var("COLORTERM").unwrap_or_default();
        let term_program = env::var("TERM_PROGRAM").unwrap_or_default();
        let kitty_window_id = env::var("KITTY_WINDOW_ID").ok();
        let is_ghostty = env::var("GHOSTTY_RESOURCES_DIR").is_ok() || term.contains("ghostty");

        let tty = atty_stdout();
        let tmux = env::var("TMUX").is_ok();
        let ssh = env::var("SSH_CONNECTION").is_ok() || env::var("SSH_TTY").is_ok();

        let unicode = !term.eq_ignore_ascii_case("dumb");
        let truecolor = colorterm == "truecolor"
            || colorterm == "24bit"
            || is_ghostty
            || term_program == "iTerm.app"
            || term_program == "WezTerm";

        let hyperlinks = is_ghostty
            || term_program == "iTerm.app"
            || term_program == "WezTerm"
            || kitty_window_id.is_some()
            || env::var("VTE_VERSION").is_ok();

        let graphics = if term.eq_ignore_ascii_case("dumb") {
            GraphicsProtocol::Off
        } else if kitty_window_id.is_some() || is_ghostty || term_program == "ghostty" {
            GraphicsProtocol::Kitty
        } else if term_program == "iTerm.app" || term_program == "WezTerm" {
            GraphicsProtocol::Iterm2
        } else {
            GraphicsProtocol::Unicode
        };

        Self {
            tty,
            term_name: if term.is_empty() {
                "unknown".to_string()
            } else {
                term
            },
            unicode,
            truecolor,
            hyperlinks,
            graphics,
            tmux,
            ssh,
        }
    }
}

fn atty_stdout() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stdout())
}
