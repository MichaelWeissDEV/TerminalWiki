use crate::ansi::{Color, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticColor {
    Foreground,
    Muted,
    Heading,
    Link,
    Code,
    Selection,
    Warning,
    Error,
    Success,
    Accent,
    Keyword,
    String,
    Comment,
    Number,
    Function,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
    Mono,
}

impl Theme {
    pub fn color(&self, semantic: SemanticColor) -> Color {
        match self {
            Theme::Dark => match semantic {
                SemanticColor::Foreground => Color::White,
                SemanticColor::Muted => Color::BrightBlack,
                SemanticColor::Heading => Color::Cyan,
                SemanticColor::Link => Color::Blue,
                SemanticColor::Code => Color::Yellow,
                SemanticColor::Selection => Color::Magenta,
                SemanticColor::Warning => Color::BrightYellow,
                SemanticColor::Error => Color::Red,
                SemanticColor::Success => Color::Green,
                SemanticColor::Accent => Color::Cyan,
                SemanticColor::Keyword => Color::Magenta,
                SemanticColor::String => Color::Green,
                SemanticColor::Comment => Color::BrightBlack,
                SemanticColor::Number => Color::Yellow,
                SemanticColor::Function => Color::Blue,
            },
            Theme::Light => match semantic {
                SemanticColor::Foreground => Color::Black,
                SemanticColor::Muted => Color::BrightBlack,
                SemanticColor::Heading => Color::Cyan,
                SemanticColor::Link => Color::Blue,
                SemanticColor::Code => Color::Yellow,
                SemanticColor::Selection => Color::Magenta,
                SemanticColor::Warning => Color::BrightYellow,
                SemanticColor::Error => Color::Red,
                SemanticColor::Success => Color::Green,
                SemanticColor::Accent => Color::Cyan,
                SemanticColor::Keyword => Color::Magenta,
                SemanticColor::String => Color::Green,
                SemanticColor::Comment => Color::BrightBlack,
                SemanticColor::Number => Color::Yellow,
                SemanticColor::Function => Color::Blue,
            },
            Theme::Mono => Color::Reset,
        }
    }
    pub fn style(&self, semantic: SemanticColor) -> Style {
        Style::new().with_fg(self.color(semantic))
    }
}
