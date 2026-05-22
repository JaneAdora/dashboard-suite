//! Minimal palette + terminal RAII, mirroring launcher-core so rsuite looks like
//! the suite it installs. Kept local to avoid a cross-workspace path dependency.
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::style::{Color, Modifier, Style};

pub const PINK: Color = Color::Rgb(0xe8, 0x8b, 0x9f);
pub const LAVENDER: Color = Color::Rgb(0xc5, 0xa3, 0xff);
pub const MAGENTA: Color = Color::Rgb(0xff, 0x6e, 0xc7);

pub fn header() -> Style {
    Style::default().fg(MAGENTA).add_modifier(Modifier::BOLD)
}
pub fn section() -> Style {
    Style::default().fg(LAVENDER).add_modifier(Modifier::BOLD)
}
pub fn active() -> Style {
    Style::default().fg(PINK).add_modifier(Modifier::BOLD)
}
pub fn dim() -> Style {
    Style::default().fg(LAVENDER).add_modifier(Modifier::DIM)
}
pub fn warn() -> Style {
    Style::default().fg(MAGENTA)
}

pub struct TerminalGuard;
impl TerminalGuard {
    pub fn enter() -> anyhow::Result<Self> {
        enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::execute!(std::io::stdout(), LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}
