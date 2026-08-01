mod palette;

pub use palette::ALL;

#[cfg(test)]
mod tests;

use ratatui::style::{Color, Modifier, Style};

use crate::sensors::{Status, Trend};

/// Minimum dashboard contract shared by runtime rendering and render tests.
/// Smaller terminals show a compact fallback instead of overlapping panels.
pub const MIN_TERMINAL_WIDTH: u16 = 36;
pub const MIN_TERMINAL_HEIGHT: u16 = 20;

/// Frame budget for the startup splash animation, shared by the runtime
/// (which drives frame timing) and the renderer (which computes shimmer
/// phase / progress-bar fill from `frame_no as f64 / SPLASH_TOTAL_FRAMES as f64`).
pub const SPLASH_TOTAL_FRAMES: u64 = 24;

/// A selectable color palette for the TUI, chosen via the `theme` config
/// field, `--theme`, or the in-app theme settings view (`t`/`F2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub id: &'static str,
    pub label: &'static str,
    /// Base terminal background, painted behind every screen so nothing is
    /// terminal-transparent.
    pub bg: Color,
    /// Primary accent (titles, app name, active highlights).
    pub accent: Color,
    /// Secondary accent (alt borders, "very unhealthy" status).
    pub accent_alt: Color,
    /// Default body text.
    pub fg: Color,
    /// Secondary / dimmed text.
    pub muted: Color,
    /// Unfocused panel border.
    pub border: Color,
    /// Focused panel border.
    pub border_focus: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub info: Color,
    /// Inverse/header background, e.g. status-line segments.
    pub highlight_bg: Color,
    pub highlight_fg: Color,
}

/// Blend `tint` into `base` by `t` (0.0 = `base`, 1.0 = `tint`). Only defined
/// for `Rgb` colors; any named/reset color passes `base` through unchanged so
/// the TTY-safe `Mono` theme is never tinted away from the user's terminal
/// palette.
fn blend(base: Color, tint: Color, t: f32) -> Color {
    match (base, tint) {
        (Color::Rgb(br, bg, bb), Color::Rgb(tr, tg, tb)) => {
            let mix = |b: u8, t_channel: u8| -> u8 {
                let b = f32::from(b);
                let t_channel = f32::from(t_channel);
                (b + (t_channel - b) * t).round().clamp(0.0, 255.0) as u8
            };
            Color::Rgb(mix(br, tr), mix(bg, tg), mix(bb, tb))
        }
        _ => base,
    }
}

impl Theme {
    pub fn default_theme() -> Theme {
        palette::DEFAULT
    }

    /// Look up a built-in theme by id, case-insensitively. Unknown ids fall
    /// back to the default theme rather than erroring, so a stale or
    /// hand-edited config value never blocks the TUI from starting.
    pub fn by_id(id: &str) -> Theme {
        ALL.iter()
            .find(|theme| theme.id.eq_ignore_ascii_case(id))
            .copied()
            .unwrap_or_else(Theme::default_theme)
    }

    pub fn index(self) -> usize {
        ALL.iter()
            .position(|theme| theme.id == self.id)
            .unwrap_or(0)
    }

    fn card_style(&self, tint: Color) -> Style {
        Style::default()
            .fg(blend(self.fg, tint, 0.12))
            .bg(blend(self.bg, tint, 0.16))
    }

    /// Centralizes the AQI/metric status → color mapping shared by card
    /// borders, status text, status badges, and the powerline status line's
    /// AQI dot — the "green/yellow/orange/red" scale mirrored from the
    /// physical AirGradient device's LED bar.
    pub fn status_color(&self, status: Status) -> Color {
        match status {
            Status::Unknown => self.border,
            Status::Good => self.success,
            Status::Moderate => self.warning,
            Status::Elevated => blend(self.warning, self.danger, 0.5),
            Status::Unhealthy => self.danger,
            Status::VeryUnhealthy => self.accent_alt,
        }
    }

    pub fn title_style(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn muted_style(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn border_style(&self) -> Style {
        Style::default().fg(self.border)
    }

    pub fn active_border_style(&self) -> Style {
        Style::default().fg(self.border_focus)
    }

    pub fn secondary_border_style(&self) -> Style {
        Style::default().fg(self.accent_alt)
    }

    pub fn good_card_style(&self) -> Style {
        self.card_style(self.success)
    }

    pub fn neutral_card_style(&self) -> Style {
        self.card_style(self.info)
    }

    pub fn warning_card_style(&self) -> Style {
        self.card_style(self.warning)
    }

    pub fn panel_style(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }

    pub fn accent_style(&self) -> Style {
        Style::default().fg(self.info).add_modifier(Modifier::BOLD)
    }

    pub fn warning_accent_style(&self) -> Style {
        Style::default()
            .fg(self.warning)
            .add_modifier(Modifier::BOLD)
    }

    pub fn trace_style(&self) -> Style {
        Style::default().fg(self.success)
    }

    pub fn header_style(&self) -> Style {
        Style::default()
            .fg(self.highlight_fg)
            .bg(self.highlight_bg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn deck_title_style(&self) -> Style {
        Style::default()
            .fg(self.fg)
            .bg(blend(self.bg, self.accent_alt, 0.4))
            .add_modifier(Modifier::BOLD)
    }

    pub fn command_style(&self) -> Style {
        Style::default()
            .fg(self.highlight_fg)
            .bg(self.warning)
            .add_modifier(Modifier::BOLD)
    }

    pub fn status_badge_style(&self, status: Status) -> Style {
        Style::default()
            .fg(self.highlight_fg)
            .bg(self.status_color(status))
            .add_modifier(Modifier::BOLD)
    }

    pub fn footer_style(&self) -> Style {
        Style::default()
            .fg(self.muted)
            .bg(blend(self.bg, self.border, 0.5))
    }

    pub fn label_style(&self) -> Style {
        Style::default().fg(blend(self.fg, self.muted, 0.5))
    }

    pub fn value_style(&self) -> Style {
        Style::default().fg(self.fg).add_modifier(Modifier::BOLD)
    }

    pub fn error_style(&self) -> Style {
        Style::default()
            .fg(self.danger)
            .add_modifier(Modifier::BOLD)
    }

    pub fn status_style(&self, status: Status) -> Style {
        match status {
            Status::Unknown => self.muted_style(),
            _ => Style::default().fg(self.status_color(status)),
        }
    }

    pub fn trend_style(&self, trend: Trend) -> Style {
        match trend {
            Trend::Unknown => self.muted_style(),
            Trend::Stable => Style::default().fg(self.muted),
            Trend::Up => Style::default().fg(self.danger),
            Trend::Down => Style::default().fg(self.success),
        }
    }
}
