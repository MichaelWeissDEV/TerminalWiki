//! RAII Terminal guard to safely enter and restore terminal state.

use std::io::{stdout, Stdout, Write};

use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use terminalwiki_core::{Error, Result};

pub struct TerminalGuard {
    _stdout: Stdout,
}

impl TerminalGuard {
    pub fn enter() -> Result<Self> {
        enable_raw_mode().map_err(|e| Error::other(format!("Failed to enable raw mode: {e}")))?;
        let mut stdout = stdout();
        execute!(stdout, EnterAlternateScreen, Hide, EnableMouseCapture)
            .map_err(|e| Error::other(format!("Failed to enter alternate screen: {e}")))?;
        Ok(TerminalGuard { _stdout: stdout })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let mut stdout = stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, Show, DisableMouseCapture);
        let _ = disable_raw_mode();
        let _ = stdout.flush();
    }
}
