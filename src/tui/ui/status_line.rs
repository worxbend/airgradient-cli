//! The top status bar, in its compact and wide-terminal variants.
//!
//! Both variants lead with a powerline dot colored by the current AQI status,
//! mirroring the physical AirGradient device's LED.

use std::time::SystemTime;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};
use tui_spinner::{FluxSpinner, Spin};

use crate::{
    sensors::{Icons, Status},
    tui::{
        app::TuiApp,
        ui::{
            ADVANCED_LAYOUT_WIDTH, components,
            format::{
                configured_url_text, fetch_status, format_duration, format_system_time_delta,
            },
            gradient, widget_block,
        },
    },
};

/// The join between the AQI dot and the rest of the bar.
///
/// The powerline triangle is a Private Use Area codepoint, so it only exists
/// in a patched font — in a plain TTY it draws as an empty box. The stand-in
/// is a geometric-shapes triangle, which is in ordinary fonts and reads the
/// same way. Which is used follows the theme, exactly as the metric icons do.
const POWERLINE_SEPARATOR: Icons = Icons {
    nerd: "\u{e0b0}",
    plain: "\u{25b6}",
};

pub(super) fn render_status_line(frame: &mut Frame<'_>, area: Rect, app: &TuiApp, now: SystemTime) {
    if area.width >= ADVANCED_LAYOUT_WIDTH {
        render_showcase_status_line(frame, area, app, now);
        return;
    }

    render_compact_status_line(frame, area, app, now);
}

/// A leading powerline-style dot, colored by AQI status (mirroring the
/// physical AirGradient device's LED — green/yellow/orange/red/purple via
/// `Theme::status_color`), followed by the original compact top-bar
/// content: app name, device URL, refresh interval, fetch status.
fn render_compact_status_line(frame: &mut Frame<'_>, area: Rect, app: &TuiApp, now: SystemTime) {
    let theme = app.theme;
    let refresh = format_duration(app.refresh_interval.as_duration());
    let block = widget_block(theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // The spinner is a widget of its own, so the bar reserves a cell for it
    // rather than trying to thread an animated glyph through a `Line`. The
    // cell is only taken while a fetch is in flight; the rest of the time the
    // text simply has it.
    let [text_area, spinner_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(if app.is_fetching { 2 } else { 0 }),
        ])
        .areas(inner);

    let mut line = vec![
        aqi_indicator_dot(app),
        aqi_indicator_separator(app),
        Span::raw(" "),
    ];
    line.extend(wordmark(app));
    line.extend([
        Span::raw("  "),
        Span::styled(configured_url_text(app), theme.muted_style()),
        Span::raw("  |  refresh "),
        Span::styled(refresh, theme.value_style()),
        Span::raw("  |  "),
        fetch_status(app, now),
    ]);

    frame.render_widget(
        Paragraph::new(Line::from(line)).style(theme.panel_style()),
        text_area,
    );
    render_fetch_spinner(frame, spinner_area, app);
}

/// The app name, coloured along a gradient from the theme's accent to its
/// alternate accent — travelling while something is happening, still when the
/// dashboard is idle.
fn wordmark(app: &TuiApp) -> Vec<Span<'static>> {
    gradient::gradient_spans(
        "AirGradient",
        app.theme.accent,
        app.theme.accent_alt,
        app.animation_frame,
        true,
    )
}

/// A braille spinner while a request is in flight.
///
/// Turned by `TuiApp::animation_frame`, which only advances while something is
/// animating — so this costs repaints during a fetch and nothing at all
/// between them.
pub(super) fn render_fetch_spinner(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    if !app.is_fetching || area.width == 0 || area.height == 0 {
        return;
    }

    frame.render_widget(
        FluxSpinner::new(app.animation_frame)
            .spin(Spin::Clockwise)
            .style(app.theme.accent_style_for_spinner()),
        area,
    );
}

/// Wide-screen identity, navigation hints, and acquisition telemetry.
fn render_showcase_status_line(frame: &mut Frame<'_>, area: Rect, app: &TuiApp, now: SystemTime) {
    let theme = app.theme;
    let tabs = Line::from(vec![
        components::tab(theme, "Air Monitor", true),
        components::tab(theme, "h History", false),
        components::tab(theme, "t Themes", false),
        components::tab(theme, "c Config", false),
    ])
    .right_aligned();
    let block = widget_block(theme)
        .border_style(theme.active_border_style())
        .title_top(tabs);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [brand, state, timing] = Layout::horizontal([
        Constraint::Length(24),
        Constraint::Min(15),
        Constraint::Length(46),
    ])
    .areas(inner);
    let mut name = vec![aqi_indicator_dot(app), Span::raw(" ")];
    name.extend(wordmark(app));
    frame.render_widget(Paragraph::new(Line::from(name)), brand);
    let status = Line::from(vec![fetch_status(app, now)]);
    frame.render_widget(Paragraph::new(status), state);
    let updated = app
        .last_success_at
        .map(|time| format_system_time_delta(time, now))
        .unwrap_or_else(|| "not yet".to_string());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Last updated: ", theme.muted_style()),
            Span::styled(updated, theme.value_style()),
            Span::styled(
                format!(
                    "  /  refresh {}",
                    format_duration(app.refresh_interval.as_duration())
                ),
                theme.muted_style(),
            ),
        ]))
        .alignment(Alignment::Right),
        timing,
    );
}

fn aqi_indicator_dot(app: &TuiApp) -> Span<'static> {
    Span::styled(
        " ● ",
        app.theme
            .badge_style(app.theme.status_color(aqi_status(app))),
    )
}

fn aqi_indicator_separator(app: &TuiApp) -> Span<'static> {
    Span::styled(
        app.theme.icon(POWERLINE_SEPARATOR),
        app.theme.status_style(aqi_status(app)),
    )
}

fn aqi_status(app: &TuiApp) -> Status {
    app.metrics().aqi().status
}
