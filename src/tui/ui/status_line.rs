//! The top status bar, in its compact and wide-terminal variants.
//!
//! Both variants lead with a powerline dot colored by the current AQI status,
//! mirroring the physical AirGradient device's LED.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    sensors::Status,
    tui::{
        app::TuiApp,
        ui::{
            ADVANCED_LAYOUT_WIDTH,
            format::{
                configured_url_text, display_metrics, fetch_status, format_duration,
                format_system_time_delta,
            },
        },
    },
};

/// Powerline right-pointing solid triangle. Renders as a smooth segment
/// join in a Nerd-Font/powerline-patched terminal font; degrades to a plain
/// glyph box in other fonts, which still reads fine as a separator.
const POWERLINE_SEPARATOR: &str = "\u{e0b0}";

pub(super) fn render_status_line(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    if area.width >= ADVANCED_LAYOUT_WIDTH {
        render_showcase_status_line(frame, area, app);
        return;
    }

    render_compact_status_line(frame, area, app);
}

/// A leading powerline-style dot, colored by AQI status (mirroring the
/// physical AirGradient device's LED — green/yellow/orange/red/purple via
/// `Theme::status_color`), followed by the original compact top-bar
/// content: app name, device URL, refresh interval, fetch status.
fn render_compact_status_line(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let refresh = format_duration(app.refresh_interval);
    let status = fetch_status(app);

    let line = Line::from(vec![
        aqi_indicator_dot(app),
        aqi_indicator_separator(app),
        Span::raw(" "),
        Span::styled("AirGradient", theme.title_style()),
        Span::raw("  "),
        Span::styled(configured_url_text(app), theme.muted_style()),
        Span::raw("  |  refresh "),
        Span::styled(refresh, theme.value_style()),
        Span::raw("  |  "),
        status,
    ]);

    frame.render_widget(
        Paragraph::new(line).style(theme.panel_style()).block(
            Block::default()
                .borders(Borders::ALL)
                .style(theme.panel_style()),
        ),
        area,
    );
}

/// Wide-terminal top bar: unchanged icon/"Air Monitor" showcase layout,
/// with an AQI-colored powerline dot added as the leading element.
fn render_showcase_status_line(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let updated = app
        .last_success_at
        .map(format_system_time_delta)
        .unwrap_or_else(|| "not yet".to_string());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(symbols::border::ROUNDED)
        .border_style(theme.border_style())
        .style(theme.panel_style());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(20),
            Constraint::Percentage(40),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            aqi_indicator_dot(app),
            aqi_indicator_separator(app),
            Span::raw("   "),
            Span::styled("⌂", theme.value_style()),
            Span::raw("   "),
            Span::styled("↻", theme.value_style()),
            Span::raw("   "),
            Span::styled("Last updated: ", theme.label_style()),
            Span::styled(updated, theme.value_style()),
        ]))
        .alignment(Alignment::Left),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(Span::styled("Air Monitor", theme.value_style()))
            .alignment(Alignment::Center),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("⚙", theme.value_style()),
            Span::raw("   "),
            Span::styled("ⓘ", theme.value_style()),
        ]))
        .alignment(Alignment::Right),
        chunks[2],
    );
}

fn aqi_indicator_dot(app: &TuiApp) -> Span<'static> {
    Span::styled(
        " ● ",
        Style::default()
            .fg(app.theme.highlight_fg)
            .bg(app.theme.status_color(aqi_status(app)))
            .add_modifier(Modifier::BOLD),
    )
}

fn aqi_indicator_separator(app: &TuiApp) -> Span<'static> {
    Span::styled(
        POWERLINE_SEPARATOR,
        Style::default().fg(app.theme.status_color(aqi_status(app))),
    )
}

fn aqi_status(app: &TuiApp) -> Status {
    display_metrics(app)
        .iter()
        .find(|metric| metric.key == "aqi")
        .map_or(Status::Unknown, |metric| metric.status)
}
