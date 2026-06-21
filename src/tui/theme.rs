use ratatui::style::{Color, Modifier, Style};

use crate::sensors::{Status, Trend};

pub const MISSING_VALUE: &str = "--";

/// Minimum dashboard contract shared by runtime rendering and render tests.
/// Smaller terminals show a compact fallback instead of overlapping panels.
pub const MIN_TERMINAL_WIDTH: u16 = 36;
pub const MIN_TERMINAL_HEIGHT: u16 = 20;

pub fn title_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

pub fn muted_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn label_style() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn value_style() -> Style {
    Style::default()
        .fg(Color::White)
        .add_modifier(Modifier::BOLD)
}

pub fn error_style() -> Style {
    Style::default()
        .fg(Color::LightRed)
        .add_modifier(Modifier::BOLD)
}

pub fn status_style(status: Status) -> Style {
    match status {
        Status::Unknown => muted_style(),
        Status::Good => Style::default().fg(Color::Green),
        Status::Moderate => Style::default().fg(Color::Yellow),
        Status::Elevated => Style::default().fg(Color::LightYellow),
        Status::Unhealthy => Style::default().fg(Color::Red),
        Status::VeryUnhealthy => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
    }
}

pub fn trend_style(trend: Trend) -> Style {
    match trend {
        Trend::Unknown => muted_style(),
        Trend::Stable => Style::default().fg(Color::Gray),
        Trend::Up => Style::default().fg(Color::LightRed),
        Trend::Down => Style::default().fg(Color::LightGreen),
    }
}
