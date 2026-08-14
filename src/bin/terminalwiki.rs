//! `terminalwiki` — the long entry point for TerminalWiki.
//!
//! This binary intentionally contains no logic of its own; it shares the exact
//! entry point with `tw`.

fn main() -> std::process::ExitCode {
    terminalwiki_cli::main()
}
