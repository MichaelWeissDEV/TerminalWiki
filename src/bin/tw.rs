//! `tw` — the short entry point for TerminalWiki.
//!
//! This binary intentionally contains no logic of its own; it shares the exact
//! entry point with `terminalwiki`.

fn main() -> std::process::ExitCode {
    terminalwiki_cli::main()
}
