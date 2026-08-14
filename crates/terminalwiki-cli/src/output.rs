use std::io::{self, Write};
use terminalwiki_core::sanitize;

pub fn is_stdout_tty() -> bool {
    // Check if stdout is a TTY. A simple heuristic is checking TERM,
    // but Rust 1.70+ has IsTerminal. Let's use the stdlib feature.
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

pub fn writeln_sanitized(out: &mut impl Write, text: &str) -> io::Result<()> {
    let sanitized = sanitize::sanitize_text(text);
    writeln!(out, "{}", sanitized)
}

pub fn print_sanitized(text: &str) {
    let _ = writeln_sanitized(&mut std::io::stdout(), text);
}
