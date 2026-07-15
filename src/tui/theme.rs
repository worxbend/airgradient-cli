use ratatui::style::{Color, Modifier, Style};

use crate::sensors::{Status, Trend};

pub const MISSING_VALUE: &str = "--";

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

macro_rules! rgb {
    ($r:expr, $g:expr, $b:expr) => {
        Color::Rgb($r, $g, $b)
    };
}

const DEFAULT: Theme = Theme {
    id: "default",
    label: "AirGradient",
    bg: rgb!(0x07, 0x0A, 0x12),
    accent: rgb!(0x5A, 0xD6, 0xFF),
    accent_alt: rgb!(0xD3, 0x6E, 0xFF),
    fg: rgb!(0xDE, 0xE9, 0xF6),
    muted: rgb!(0x69, 0x71, 0x82),
    border: rgb!(0x3A, 0x44, 0x59),
    border_focus: rgb!(0x5C, 0xE1, 0xE6),
    success: rgb!(0x50, 0xFF, 0xA6),
    warning: rgb!(0xFF, 0xC7, 0x5F),
    danger: rgb!(0xFF, 0x56, 0x5C),
    info: rgb!(0x49, 0xDE, 0xFF),
    highlight_bg: rgb!(0x5C, 0xE1, 0xE6),
    highlight_fg: rgb!(0x05, 0x08, 0x0E),
};

const CLAUDE: Theme = Theme {
    id: "claude",
    label: "Claude",
    bg: rgb!(0x1B, 0x19, 0x16),
    accent: rgb!(0xD9, 0x77, 0x57),
    accent_alt: rgb!(0xE8, 0xC5, 0x9E),
    fg: rgb!(0xEC, 0xE8, 0xE1),
    muted: rgb!(0x8A, 0x86, 0x7D),
    border: rgb!(0x4A, 0x46, 0x40),
    border_focus: rgb!(0xD9, 0x77, 0x57),
    success: rgb!(0x87, 0xB3, 0x7B),
    warning: rgb!(0xE0, 0xB4, 0x4C),
    danger: rgb!(0xE0, 0x6C, 0x5F),
    info: rgb!(0x7B, 0xA9, 0xC7),
    highlight_bg: rgb!(0xD9, 0x77, 0x57),
    highlight_fg: rgb!(0x1B, 0x19, 0x16),
};

const CODEX: Theme = Theme {
    id: "codex",
    label: "Codex",
    bg: rgb!(0x0A, 0x14, 0x12),
    accent: rgb!(0x37, 0xE0, 0xB0),
    accent_alt: rgb!(0x8A, 0xB4, 0xFF),
    fg: rgb!(0xE3, 0xE8, 0xE6),
    muted: rgb!(0x6B, 0x76, 0x74),
    border: rgb!(0x2A, 0x33, 0x31),
    border_focus: rgb!(0x37, 0xE0, 0xB0),
    success: rgb!(0x37, 0xE0, 0xB0),
    warning: rgb!(0xF2, 0xC9, 0x4C),
    danger: rgb!(0xF2, 0x5F, 0x5F),
    info: rgb!(0x8A, 0xB4, 0xFF),
    highlight_bg: rgb!(0x37, 0xE0, 0xB0),
    highlight_fg: rgb!(0x0A, 0x14, 0x12),
};

const BTOP: Theme = Theme {
    id: "btop",
    label: "Btop",
    bg: rgb!(0x0A, 0x14, 0x0A),
    accent: rgb!(0x6A, 0xE0, 0x5A),
    accent_alt: rgb!(0xF0, 0xE0, 0x50),
    fg: rgb!(0xD4, 0xE6, 0xD4),
    muted: rgb!(0x5A, 0x6A, 0x5A),
    border: rgb!(0x30, 0x40, 0x30),
    border_focus: rgb!(0x6A, 0xE0, 0x5A),
    success: rgb!(0x6A, 0xE0, 0x5A),
    warning: rgb!(0xF0, 0xE0, 0x50),
    danger: rgb!(0xE0, 0x50, 0x50),
    info: rgb!(0x50, 0xC0, 0xE0),
    highlight_bg: rgb!(0x6A, 0xE0, 0x5A),
    highlight_fg: rgb!(0x0A, 0x14, 0x0A),
};

const NORD: Theme = Theme {
    id: "nord",
    label: "Nord",
    bg: rgb!(0x2E, 0x34, 0x40),
    accent: rgb!(0x88, 0xC0, 0xD0),
    accent_alt: rgb!(0x81, 0xA1, 0xC1),
    fg: rgb!(0xE5, 0xE9, 0xF0),
    muted: rgb!(0x61, 0x6E, 0x88),
    border: rgb!(0x3B, 0x42, 0x52),
    border_focus: rgb!(0x88, 0xC0, 0xD0),
    success: rgb!(0xA3, 0xBE, 0x8C),
    warning: rgb!(0xEB, 0xCB, 0x8B),
    danger: rgb!(0xBF, 0x61, 0x6A),
    info: rgb!(0x81, 0xA1, 0xC1),
    highlight_bg: rgb!(0x88, 0xC0, 0xD0),
    highlight_fg: rgb!(0x2E, 0x34, 0x40),
};

const DRACULA: Theme = Theme {
    id: "dracula",
    label: "Dracula",
    bg: rgb!(0x28, 0x2A, 0x36),
    accent: rgb!(0xBD, 0x93, 0xF9),
    accent_alt: rgb!(0xFF, 0x79, 0xC6),
    fg: rgb!(0xF8, 0xF8, 0xF2),
    muted: rgb!(0x62, 0x72, 0xA4),
    border: rgb!(0x3A, 0x3D, 0x52),
    border_focus: rgb!(0xBD, 0x93, 0xF9),
    success: rgb!(0x50, 0xFA, 0x7B),
    warning: rgb!(0xF1, 0xFA, 0x8C),
    danger: rgb!(0xFF, 0x55, 0x55),
    info: rgb!(0x8B, 0xE9, 0xFD),
    highlight_bg: rgb!(0xBD, 0x93, 0xF9),
    highlight_fg: rgb!(0x1E, 0x1F, 0x29),
};

const GRUVBOX: Theme = Theme {
    id: "gruvbox",
    label: "Gruvbox",
    bg: rgb!(0x28, 0x28, 0x28),
    accent: rgb!(0xFE, 0x80, 0x19),
    accent_alt: rgb!(0xD3, 0x86, 0x9B),
    fg: rgb!(0xEB, 0xDB, 0xB2),
    muted: rgb!(0x92, 0x83, 0x74),
    border: rgb!(0x3C, 0x38, 0x36),
    border_focus: rgb!(0xFE, 0x80, 0x19),
    success: rgb!(0xB8, 0xBB, 0x26),
    warning: rgb!(0xFA, 0xBD, 0x2F),
    danger: rgb!(0xFB, 0x49, 0x34),
    info: rgb!(0x83, 0xA5, 0x98),
    highlight_bg: rgb!(0xFE, 0x80, 0x19),
    highlight_fg: rgb!(0x28, 0x28, 0x28),
};

const SOLARIZED_DARK: Theme = Theme {
    id: "solarized-dark",
    label: "Solarized Dark",
    bg: rgb!(0x00, 0x2B, 0x36),
    accent: rgb!(0x26, 0x8B, 0xD2),
    accent_alt: rgb!(0x2A, 0xA1, 0x98),
    fg: rgb!(0x93, 0xA1, 0xA1),
    muted: rgb!(0x58, 0x6E, 0x75),
    border: rgb!(0x07, 0x36, 0x42),
    border_focus: rgb!(0x26, 0x8B, 0xD2),
    success: rgb!(0x85, 0x99, 0x00),
    warning: rgb!(0xB5, 0x89, 0x00),
    danger: rgb!(0xDC, 0x32, 0x2F),
    info: rgb!(0x2A, 0xA1, 0x98),
    highlight_bg: rgb!(0x26, 0x8B, 0xD2),
    highlight_fg: rgb!(0x00, 0x2B, 0x36),
};

const MONOKAI: Theme = Theme {
    id: "monokai",
    label: "Monokai",
    bg: rgb!(0x27, 0x28, 0x22),
    accent: rgb!(0xA6, 0xE2, 0x2E),
    accent_alt: rgb!(0xAE, 0x81, 0xFF),
    fg: rgb!(0xF8, 0xF8, 0xF2),
    muted: rgb!(0x75, 0x71, 0x5E),
    border: rgb!(0x3E, 0x3D, 0x32),
    border_focus: rgb!(0xA6, 0xE2, 0x2E),
    success: rgb!(0xA6, 0xE2, 0x2E),
    warning: rgb!(0xE6, 0xDB, 0x74),
    danger: rgb!(0xF9, 0x26, 0x72),
    info: rgb!(0x66, 0xD9, 0xEF),
    highlight_bg: rgb!(0xA6, 0xE2, 0x2E),
    highlight_fg: rgb!(0x27, 0x28, 0x22),
};

const ONE_DARK: Theme = Theme {
    id: "one-dark",
    label: "One Dark",
    bg: rgb!(0x28, 0x2C, 0x34),
    accent: rgb!(0x61, 0xAF, 0xEF),
    accent_alt: rgb!(0xC6, 0x78, 0xDD),
    fg: rgb!(0xAB, 0xB2, 0xBF),
    muted: rgb!(0x5C, 0x63, 0x70),
    border: rgb!(0x3E, 0x44, 0x51),
    border_focus: rgb!(0x61, 0xAF, 0xEF),
    success: rgb!(0x98, 0xC3, 0x79),
    warning: rgb!(0xE5, 0xC0, 0x7B),
    danger: rgb!(0xE0, 0x6C, 0x75),
    info: rgb!(0x56, 0xB6, 0xC2),
    highlight_bg: rgb!(0x61, 0xAF, 0xEF),
    highlight_fg: rgb!(0x28, 0x2C, 0x34),
};

const TOKYO_NIGHT: Theme = Theme {
    id: "tokyo-night",
    label: "Tokyo Night",
    bg: rgb!(0x1A, 0x1B, 0x26),
    accent: rgb!(0x7A, 0xA2, 0xF7),
    accent_alt: rgb!(0xBB, 0x9A, 0xF7),
    fg: rgb!(0xC0, 0xCA, 0xF5),
    muted: rgb!(0x56, 0x5F, 0x89),
    border: rgb!(0x24, 0x28, 0x3B),
    border_focus: rgb!(0x7A, 0xA2, 0xF7),
    success: rgb!(0x9E, 0xCE, 0x6A),
    warning: rgb!(0xE0, 0xAF, 0x68),
    danger: rgb!(0xF7, 0x76, 0x8E),
    info: rgb!(0x7D, 0xCF, 0xFF),
    highlight_bg: rgb!(0x7A, 0xA2, 0xF7),
    highlight_fg: rgb!(0x1A, 0x1B, 0x26),
};

const CATPPUCCIN_MOCHA: Theme = Theme {
    id: "catppuccin-mocha",
    label: "Catppuccin Mocha",
    bg: rgb!(0x1E, 0x1E, 0x2E),
    accent: rgb!(0xCB, 0xA6, 0xF7),
    accent_alt: rgb!(0x89, 0xB4, 0xFA),
    fg: rgb!(0xCD, 0xD6, 0xF4),
    muted: rgb!(0xA6, 0xAD, 0xC8),
    border: rgb!(0x31, 0x32, 0x44),
    border_focus: rgb!(0xCB, 0xA6, 0xF7),
    success: rgb!(0xA6, 0xE3, 0xA1),
    warning: rgb!(0xF9, 0xE2, 0xAF),
    danger: rgb!(0xF3, 0x8B, 0xA8),
    info: rgb!(0x89, 0xDC, 0xEB),
    highlight_bg: rgb!(0xCB, 0xA6, 0xF7),
    highlight_fg: rgb!(0x1E, 0x1E, 0x2E),
};

const ROSE_PINE: Theme = Theme {
    id: "rose-pine",
    label: "Rose Pine",
    bg: rgb!(0x19, 0x17, 0x24),
    accent: rgb!(0xC4, 0xA7, 0xE7),
    accent_alt: rgb!(0xEB, 0xBC, 0xBA),
    fg: rgb!(0xE0, 0xDE, 0xF4),
    muted: rgb!(0x6E, 0x6A, 0x86),
    border: rgb!(0x26, 0x23, 0x3A),
    border_focus: rgb!(0xC4, 0xA7, 0xE7),
    success: rgb!(0x9C, 0xCF, 0xD8),
    warning: rgb!(0xF6, 0xC1, 0x77),
    danger: rgb!(0xEB, 0x6F, 0x92),
    info: rgb!(0x9C, 0xCF, 0xD8),
    highlight_bg: rgb!(0xC4, 0xA7, 0xE7),
    highlight_fg: rgb!(0x19, 0x17, 0x24),
};

const MONO: Theme = Theme {
    id: "mono",
    label: "Mono (TTY-safe)",
    // Reset (not a fixed color) so this theme never overrides the user's
    // own terminal background — that's the point of a "TTY-safe" theme.
    bg: Color::Reset,
    accent: Color::White,
    accent_alt: Color::Gray,
    fg: Color::White,
    muted: Color::DarkGray,
    border: Color::DarkGray,
    border_focus: Color::White,
    success: Color::Green,
    warning: Color::Yellow,
    danger: Color::Red,
    info: Color::Cyan,
    highlight_bg: Color::White,
    highlight_fg: Color::Black,
};

const AYU_DARK: Theme = Theme {
    id: "ayu-dark",
    label: "Ayu Dark",
    bg: rgb!(0x0A, 0x0E, 0x14),
    accent: rgb!(0xFF, 0xB4, 0x54),
    accent_alt: rgb!(0x59, 0xC2, 0xFF),
    fg: rgb!(0xB3, 0xB1, 0xAD),
    muted: rgb!(0x5C, 0x67, 0x73),
    border: rgb!(0x13, 0x17, 0x21),
    border_focus: rgb!(0xFF, 0xB4, 0x54),
    success: rgb!(0x91, 0xB3, 0x62),
    warning: rgb!(0xE6, 0xB4, 0x50),
    danger: rgb!(0xD9, 0x57, 0x57),
    info: rgb!(0x59, 0xC2, 0xFF),
    highlight_bg: rgb!(0xFF, 0xB4, 0x54),
    highlight_fg: rgb!(0x0A, 0x0E, 0x14),
};

const EVERFOREST_DARK: Theme = Theme {
    id: "everforest-dark",
    label: "Everforest Dark",
    bg: rgb!(0x2D, 0x35, 0x3B),
    accent: rgb!(0xA7, 0xC0, 0x80),
    accent_alt: rgb!(0x83, 0xC0, 0x92),
    fg: rgb!(0xD3, 0xC6, 0xAA),
    muted: rgb!(0x85, 0x92, 0x89),
    border: rgb!(0x47, 0x52, 0x58),
    border_focus: rgb!(0xA7, 0xC0, 0x80),
    success: rgb!(0xA7, 0xC0, 0x80),
    warning: rgb!(0xDB, 0xBC, 0x7F),
    danger: rgb!(0xE6, 0x7E, 0x80),
    info: rgb!(0x7F, 0xBB, 0xB3),
    highlight_bg: rgb!(0xA7, 0xC0, 0x80),
    highlight_fg: rgb!(0x2D, 0x35, 0x3B),
};

const KANAGAWA: Theme = Theme {
    id: "kanagawa",
    label: "Kanagawa",
    bg: rgb!(0x1F, 0x1F, 0x28),
    accent: rgb!(0x7E, 0x9C, 0xD8),
    accent_alt: rgb!(0x95, 0x7F, 0xB8),
    fg: rgb!(0xDC, 0xD7, 0xBA),
    muted: rgb!(0x72, 0x71, 0x69),
    border: rgb!(0x36, 0x36, 0x46),
    border_focus: rgb!(0x7E, 0x9C, 0xD8),
    success: rgb!(0x98, 0xBB, 0x6C),
    warning: rgb!(0xE6, 0xC3, 0x84),
    danger: rgb!(0xC3, 0x40, 0x43),
    info: rgb!(0x7F, 0xB4, 0xCA),
    highlight_bg: rgb!(0x7E, 0x9C, 0xD8),
    highlight_fg: rgb!(0x1F, 0x1F, 0x28),
};

const SYNTHWAVE_84: Theme = Theme {
    id: "synthwave-84",
    label: "Synthwave '84",
    bg: rgb!(0x26, 0x23, 0x35),
    accent: rgb!(0xFF, 0x7E, 0xDB),
    accent_alt: rgb!(0x36, 0xF9, 0xF6),
    fg: rgb!(0xF4, 0xEE, 0xE4),
    muted: rgb!(0x84, 0x8B, 0xBD),
    border: rgb!(0x49, 0x54, 0x95),
    border_focus: rgb!(0xFF, 0x7E, 0xDB),
    success: rgb!(0x72, 0xF1, 0xB8),
    warning: rgb!(0xFE, 0xDE, 0x5D),
    danger: rgb!(0xFE, 0x44, 0x50),
    info: rgb!(0x36, 0xF9, 0xF6),
    highlight_bg: rgb!(0xFF, 0x7E, 0xDB),
    highlight_fg: rgb!(0x26, 0x23, 0x35),
};

const GITHUB_DARK: Theme = Theme {
    id: "github-dark",
    label: "GitHub Dark",
    bg: rgb!(0x0D, 0x11, 0x17),
    accent: rgb!(0x58, 0xA6, 0xFF),
    accent_alt: rgb!(0xBC, 0x8C, 0xFF),
    fg: rgb!(0xC9, 0xD1, 0xD9),
    muted: rgb!(0x8B, 0x94, 0x9E),
    border: rgb!(0x30, 0x36, 0x3D),
    border_focus: rgb!(0x58, 0xA6, 0xFF),
    success: rgb!(0x3F, 0xB9, 0x50),
    warning: rgb!(0xD2, 0x99, 0x22),
    danger: rgb!(0xF8, 0x51, 0x49),
    info: rgb!(0x79, 0xC0, 0xFF),
    highlight_bg: rgb!(0x58, 0xA6, 0xFF),
    highlight_fg: rgb!(0x0D, 0x11, 0x17),
};

const NIGHTFOX: Theme = Theme {
    id: "nightfox",
    label: "Nightfox",
    bg: rgb!(0x19, 0x23, 0x30),
    accent: rgb!(0x71, 0x9C, 0xD6),
    accent_alt: rgb!(0xC2, 0x96, 0xEB),
    fg: rgb!(0xCD, 0xCE, 0xCF),
    muted: rgb!(0x71, 0x83, 0x9B),
    border: rgb!(0x29, 0x39, 0x4F),
    border_focus: rgb!(0x71, 0x9C, 0xD6),
    success: rgb!(0x81, 0xB2, 0x9A),
    warning: rgb!(0xDB, 0xC0, 0x74),
    danger: rgb!(0xC9, 0x4F, 0x6D),
    info: rgb!(0x63, 0xCD, 0xCF),
    highlight_bg: rgb!(0x71, 0x9C, 0xD6),
    highlight_fg: rgb!(0x19, 0x23, 0x30),
};

pub const ALL: &[Theme] = &[
    DEFAULT,
    CLAUDE,
    CODEX,
    BTOP,
    NORD,
    DRACULA,
    GRUVBOX,
    SOLARIZED_DARK,
    MONOKAI,
    ONE_DARK,
    TOKYO_NIGHT,
    CATPPUCCIN_MOCHA,
    ROSE_PINE,
    AYU_DARK,
    EVERFOREST_DARK,
    KANAGAWA,
    SYNTHWAVE_84,
    GITHUB_DARK,
    NIGHTFOX,
    MONO,
];

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
        DEFAULT
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn by_id_falls_back_to_default_for_unknown_name() {
        assert_eq!(Theme::by_id("does-not-exist"), Theme::default_theme());
    }

    #[test]
    fn by_id_is_case_insensitive() {
        assert_eq!(Theme::by_id("BTOP").id, "btop");
        assert_eq!(Theme::by_id("Nord").id, "nord");
    }

    #[test]
    fn by_id_resolves_default_id() {
        assert_eq!(Theme::by_id("default"), DEFAULT);
        assert_eq!(Theme::by_id("DEFAULT"), DEFAULT);
    }

    #[test]
    fn all_themes_have_unique_ids() {
        let mut ids: Vec<&str> = ALL.iter().map(|theme| theme.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ALL.len());
    }

    #[test]
    fn there_are_twenty_built_in_themes() {
        assert_eq!(ALL.len(), 20);
    }

    #[test]
    fn index_matches_position_in_all() {
        for (i, theme) in ALL.iter().enumerate() {
            assert_eq!(theme.index(), i);
        }
    }

    #[test]
    fn mono_theme_does_not_override_terminal_background() {
        assert_eq!(MONO.bg, Color::Reset);
    }

    #[test]
    fn blend_at_zero_returns_base_and_at_one_returns_tint() {
        let base = Color::Rgb(10, 20, 30);
        let tint = Color::Rgb(200, 150, 100);
        assert_eq!(blend(base, tint, 0.0), base);
        assert_eq!(blend(base, tint, 1.0), tint);
    }

    #[test]
    fn blend_passes_through_non_rgb_colors() {
        assert_eq!(blend(Color::Reset, Color::Rgb(1, 2, 3), 0.5), Color::Reset);
    }

    #[test]
    fn status_color_covers_every_status_distinctly_for_default_theme() {
        let theme = Theme::default_theme();
        let colors = [
            theme.status_color(Status::Unknown),
            theme.status_color(Status::Good),
            theme.status_color(Status::Moderate),
            theme.status_color(Status::Elevated),
            theme.status_color(Status::Unhealthy),
            theme.status_color(Status::VeryUnhealthy),
        ];
        for (i, a) in colors.iter().enumerate() {
            for (j, b) in colors.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "status colors {i} and {j} should differ");
                }
            }
        }
    }
}
