//! ANSI terminal colour and style primitives (spec §65).
//!
//! `Style::apply()` sanitizes text before wrapping it with escape codes so no
//! untrusted content can smuggle control sequences through styled output
//! (spec §43).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    Reset,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Rgb(u8, u8, u8),
    Index(u8),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
    pub dim: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_fg(mut self, c: Color) -> Self {
        self.fg = Some(c);
        self
    }

    pub fn with_bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub fn with_italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub fn with_dim(mut self) -> Self {
        self.dim = true;
        self
    }

    pub fn with_underline(mut self) -> Self {
        self.underline = true;
        self
    }

    /// Wraps `text` with the ANSI codes for this style.
    ///
    /// Text is sanitized first so no untrusted content can inject escape
    /// sequences through a styled span (spec §43).
    pub fn apply(&self, text: &str) -> String {
        // Sanitize first — only our own escape codes may reach the terminal.
        let safe = terminalwiki_core::sanitize::sanitize_line(text);
        let mut prefix = String::new();
        if self.bold {
            prefix.push_str("\x1b[1m");
        }
        if self.dim {
            prefix.push_str("\x1b[2m");
        }
        if self.italic {
            prefix.push_str("\x1b[3m");
        }
        if self.underline {
            prefix.push_str("\x1b[4m");
        }
        if self.strikethrough {
            prefix.push_str("\x1b[9m");
        }
        if let Some(c) = self.fg {
            match c {
                Color::Reset => prefix.push_str("\x1b[39m"),
                Color::Black => prefix.push_str("\x1b[30m"),
                Color::Red => prefix.push_str("\x1b[31m"),
                Color::Green => prefix.push_str("\x1b[32m"),
                Color::Yellow => prefix.push_str("\x1b[33m"),
                Color::Blue => prefix.push_str("\x1b[34m"),
                Color::Magenta => prefix.push_str("\x1b[35m"),
                Color::Cyan => prefix.push_str("\x1b[36m"),
                Color::White => prefix.push_str("\x1b[37m"),
                Color::BrightBlack => prefix.push_str("\x1b[90m"),
                Color::BrightRed => prefix.push_str("\x1b[91m"),
                Color::BrightGreen => prefix.push_str("\x1b[92m"),
                Color::BrightYellow => prefix.push_str("\x1b[93m"),
                Color::BrightBlue => prefix.push_str("\x1b[94m"),
                Color::BrightMagenta => prefix.push_str("\x1b[95m"),
                Color::BrightCyan => prefix.push_str("\x1b[96m"),
                Color::BrightWhite => prefix.push_str("\x1b[97m"),
                Color::Rgb(r, g, b) => {
                    prefix.push_str(&format!("\x1b[38;2;{};{};{}m", r, g, b))
                }
                Color::Index(i) => prefix.push_str(&format!("\x1b[38;5;{}m", i)),
            }
        }
        if prefix.is_empty() {
            safe
        } else {
            format!("{}{}\x1b[0m", prefix, safe)
        }
    }

    /// True when this style has no visible effect.
    pub fn is_plain(&self) -> bool {
        self.fg.is_none()
            && !self.bold
            && !self.italic
            && !self.dim
            && !self.underline
            && !self.strikethrough
    }
}

/// Returns whether ANSI output should be emitted given `mode`.
///
/// Respects `NO_COLOR` (spec §64) and performs TTY detection.
pub fn should_use_color(mode: ColorMode) -> bool {
    match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => {
            if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
                return false;
            }
            use std::io::IsTerminal;
            std::io::stdout().is_terminal()
        }
    }
}
