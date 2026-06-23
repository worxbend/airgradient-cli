use ratatui::style::{Color, Modifier, Style};

use crate::sensors::{Status, Trend};

pub const MISSING_VALUE: &str = "--";

/// Minimum dashboard contract shared by runtime rendering and render tests.
/// Smaller terminals show a compact fallback instead of overlapping panels.
pub const MIN_TERMINAL_WIDTH: u16 = 36;
pub const MIN_TERMINAL_HEIGHT: u16 = 20;

pub fn title_style() -> Style {
    Style::default()
        .fg(Color::Rgb(90, 214, 255))
        .add_modifier(Modifier::BOLD)
}

pub fn muted_style() -> Style {
    Style::default().fg(Color::Rgb(105, 113, 130))
}

pub fn border_style() -> Style {
    Style::default().fg(Color::Rgb(58, 68, 89))
}

pub fn active_border_style() -> Style {
    Style::default().fg(Color::Rgb(92, 225, 230))
}

pub fn secondary_border_style() -> Style {
    Style::default().fg(Color::Rgb(211, 110, 255))
}

pub fn good_card_style() -> Style {
    Style::default()
        .fg(Color::Rgb(238, 245, 241))
        .bg(Color::Rgb(41, 58, 54))
}

pub fn neutral_card_style() -> Style {
    Style::default()
        .fg(Color::Rgb(238, 241, 247))
        .bg(Color::Rgb(48, 56, 75))
}

pub fn warning_card_style() -> Style {
    Style::default()
        .fg(Color::Rgb(244, 238, 218))
        .bg(Color::Rgb(68, 61, 44))
}

pub fn panel_style() -> Style {
    Style::default()
        .fg(Color::Rgb(222, 233, 246))
        .bg(Color::Rgb(7, 10, 18))
}

pub fn accent_style() -> Style {
    Style::default()
        .fg(Color::Rgb(73, 222, 255))
        .add_modifier(Modifier::BOLD)
}

pub fn warning_accent_style() -> Style {
    Style::default()
        .fg(Color::Rgb(255, 199, 95))
        .add_modifier(Modifier::BOLD)
}

pub fn trace_style() -> Style {
    Style::default().fg(Color::Rgb(80, 255, 166))
}

pub fn header_style() -> Style {
    Style::default()
        .fg(Color::Rgb(5, 8, 14))
        .bg(Color::Rgb(92, 225, 230))
        .add_modifier(Modifier::BOLD)
}

pub fn deck_title_style() -> Style {
    Style::default()
        .fg(Color::Rgb(255, 255, 255))
        .bg(Color::Rgb(40, 52, 78))
        .add_modifier(Modifier::BOLD)
}

pub fn command_style() -> Style {
    Style::default()
        .fg(Color::Rgb(8, 10, 18))
        .bg(Color::Rgb(255, 199, 95))
        .add_modifier(Modifier::BOLD)
}

pub fn status_badge_style(status: Status) -> Style {
    let background = match status {
        Status::Unknown => Color::Rgb(58, 68, 89),
        Status::Good => Color::Rgb(80, 255, 166),
        Status::Moderate => Color::Rgb(255, 224, 102),
        Status::Elevated => Color::Rgb(255, 168, 76),
        Status::Unhealthy => Color::Rgb(255, 86, 92),
        Status::VeryUnhealthy => Color::Rgb(211, 110, 255),
    };

    Style::default()
        .fg(Color::Rgb(5, 8, 14))
        .bg(background)
        .add_modifier(Modifier::BOLD)
}

pub fn footer_style() -> Style {
    Style::default()
        .fg(Color::Rgb(178, 190, 213))
        .bg(Color::Rgb(10, 14, 24))
}

pub fn label_style() -> Style {
    Style::default().fg(Color::Rgb(145, 154, 173))
}

pub fn value_style() -> Style {
    Style::default()
        .fg(Color::Rgb(245, 248, 255))
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
