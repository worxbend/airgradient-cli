//! Frame rendering for the TUI.
//!
//! This module owns only the top-level dispatch: which screen the current
//! [`TuiApp`] state maps to, and how the dashboard screen's vertical bands are
//! sized. Every band delegates to a sibling module that owns one surface:
//!
//! - [`status_line`] — the top bar
//! - [`dashboard`] / [`air_monitor`] — the compact and wide metric layouts
//! - [`chrome`] — error panel, footer, command-palette bar
//! - [`settings`] — the full-screen theme picker and config editor
//! - [`splash`] — the startup animation
//! - [`format`] — value/trend/duration formatting shared by all of the above

mod air_monitor;
mod air_monitor_card;
mod chrome;
mod dashboard;
mod format;
mod hit;
mod leader;
mod settings;
mod splash;
mod status_line;

pub use hit::{HitMap, HitTarget};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::tui::{
    app::{TuiApp, View},
    theme::{self, Theme},
};

/// Terminal width at or above which the wide "Air Monitor" showcase layout
/// replaces the compact one. The showcase rows need roughly this much width
/// before the metric cards stop truncating their labels.
pub(super) const ADVANCED_LAYOUT_WIDTH: u16 = 112;

/// Terminal height the showcase dashboard needs for its fixed row stack
/// (URL line, hero row, gas row, PM row, and the spacers between them).
pub(super) const ADVANCED_LAYOUT_HEIGHT: u16 = 28;

/// Renders one frame of whatever screen the app is currently showing.
///
/// Screens are mutually exclusive and checked in priority order: splash wins
/// over everything, then the full-screen settings views, then the size guard,
/// and finally the dashboard with its optional error and palette bands.
pub fn draw(frame: &mut Frame<'_>, app: &TuiApp) {
    draw_with_hits(frame, app, &mut HitMap::default());
}

/// Draws a frame and records where the clickable things ended up.
///
/// The production runtime uses this so mouse clicks can be resolved against
/// the frame the user is actually looking at; callers that do not handle mouse
/// input can use [`draw`] and ignore the map.
pub fn draw_with_hits(frame: &mut Frame<'_>, app: &TuiApp, hits: &mut HitMap) {
    hits.clear();

    let area = frame.area();

    // A terminal can report a zero-sized viewport — during a resize, or from a
    // PTY whose window size was never set. Every renderer below assumes it has
    // at least one cell to draw into, and ratatui panics on a write outside
    // the buffer, so there is nothing to do but return.
    if area.width == 0 || area.height == 0 {
        return;
    }

    let theme = app.theme;

    // Every screen paints an opaque, theme-colored backdrop first so
    // nothing is ever terminal-transparent.
    frame.render_widget(Paragraph::new("").style(theme.panel_style()), area);

    if let Some(frame_no) = app.splash_frame {
        splash::render_splash(frame, area, theme, frame_no);
        return;
    }

    match app.view {
        View::ThemeSettings => {
            settings::render_theme_settings(frame, area, app, hits);
            return;
        }
        View::ConfigEditor => {
            settings::render_config_editor(frame, area, app, hits);
            return;
        }
        View::CommandPalette | View::Dashboard => {}
    }

    if area.width < theme::MIN_TERMINAL_WIDTH || area.height < theme::MIN_TERMINAL_HEIGHT {
        render_terminal_too_small(frame, area, theme);
        return;
    }

    render_dashboard_screen(frame, area, app);

    // Drawn last so the which-key popup sits above the dashboard.
    if app.leader_pending {
        leader::render_leader_popup(frame, area, app);
    }
}

/// Splits the dashboard screen into its five vertical bands and fills them.
/// The error and palette bands collapse to zero height when inactive, so the
/// dashboard reclaims that space instead of leaving a gap.
fn render_dashboard_screen(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let has_error = app.current_error.is_some();
    let has_palette = app.view == View::CommandPalette;
    let error_height = if has_error { 5 } else { 0 };
    let palette_height = if has_palette { 1 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(error_height),
            Constraint::Length(3),
            Constraint::Length(palette_height),
        ])
        .split(area);

    status_line::render_status_line(frame, chunks[0], app);
    dashboard::render_dashboard(frame, chunks[1], app);

    if has_error {
        chrome::render_error(frame, chunks[2], app);
    }

    chrome::render_footer(frame, chunks[3], app.theme);

    if has_palette {
        chrome::render_command_palette(frame, chunks[4], app);
    }
}

fn render_terminal_too_small(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let lines = vec![
        Line::from(Span::styled("Terminal too small", theme.error_style())),
        Line::from(Span::styled(
            format!(
                "Minimum {}x{}",
                theme::MIN_TERMINAL_WIDTH,
                theme::MIN_TERMINAL_HEIGHT
            ),
            theme.muted_style(),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.panel_style())
            .wrap(Wrap { trim: true }),
        area,
    );
}
