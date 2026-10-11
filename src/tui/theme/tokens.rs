//! Derived cockpit surfaces and interaction states. Palettes remain data-only.

use ratatui::style::{Color, Modifier, Style};

use super::{Theme, blend};
use crate::sensors::Status;

/// Visual intent, independent of a particular widget or sensor threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisualState {
    Normal,
    Focused,
    Selected,
    Active,
    Disabled,
    Success,
    Warning,
    Error,
    Info,
}

impl Theme {
    pub fn surface(&self) -> Color {
        blend(self.bg, self.fg, 0.035)
    }

    pub fn surface_hi(&self) -> Color {
        blend(self.bg, self.accent, 0.12)
    }

    pub fn track(&self) -> Color {
        // Reset is safe as a background but not as an empty bar foreground.
        if self.bg == Color::Reset {
            self.border
        } else {
            blend(self.bg, self.fg, 0.18)
        }
    }

    pub fn page_style(&self) -> Style {
        Style::default().fg(self.fg).bg(self.bg)
    }

    pub fn surface_style(&self) -> Style {
        Style::default().fg(self.fg).bg(self.surface())
    }

    pub fn raised_style(&self) -> Style {
        Style::default().fg(self.fg).bg(self.surface_hi())
    }

    pub fn selected_style(&self) -> Style {
        self.raised_style()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn key_style(&self) -> Style {
        self.raised_style()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn badge_style(&self, color: Color) -> Style {
        self.raised_style().fg(color).add_modifier(Modifier::BOLD)
    }

    pub fn card_surface(&self, status: Status) -> Style {
        Style::default()
            .fg(self.fg)
            .bg(blend(self.surface(), self.status_color(status), 0.045))
    }

    pub fn state_color(&self, state: VisualState) -> Color {
        match state {
            VisualState::Normal => self.fg,
            VisualState::Focused => self.border_focus,
            VisualState::Selected | VisualState::Active => self.accent,
            VisualState::Disabled => self.muted,
            VisualState::Success => self.success,
            VisualState::Warning => self.warning,
            VisualState::Error => self.danger,
            VisualState::Info => self.info,
        }
    }

    pub fn state_style(&self, state: VisualState) -> Style {
        match state {
            VisualState::Selected => self.selected_style(),
            VisualState::Focused | VisualState::Active => Style::default()
                .fg(self.state_color(state))
                .add_modifier(Modifier::BOLD),
            _ => Style::default().fg(self.state_color(state)),
        }
    }

    /// The complete set of fills allowed in a frame; used by buffer regressions.
    pub fn is_background(&self, color: Color) -> bool {
        [
            self.bg,
            self.surface(),
            self.surface_hi(),
            self.highlight_bg,
        ]
        .contains(&color)
            || [
                Status::Unknown,
                Status::Good,
                Status::Moderate,
                Status::Elevated,
                Status::Unhealthy,
                Status::VeryUnhealthy,
            ]
            .into_iter()
            .any(|status| self.card_surface(status).bg == Some(color))
    }
}
