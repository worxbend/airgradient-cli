//! The wide "Air Monitor" showcase dashboard.
//!
//! This module owns row composition only — which metric goes where, and how
//! each band is sized. Drawing an individual card lives in
//! [`super::air_monitor_card`].

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Padding, Paragraph, Wrap},
};

use crate::{
    sensors::{Metric, Metrics},
    tui::{
        app::TuiApp,
        theme::Theme,
        ui::{
            WIDGET_GAP,
            air_monitor_card::{
                CardSpec, render_air_monitor_aqi_card, render_air_monitor_metric_card,
            },
            format::configured_url_text,
            vertical_gap, widget_block,
        },
    },
};

pub(super) fn render_advanced_dashboard(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    render_showcase_backdrop(frame, area, theme);

    // Fixed row heights with 1-cell spacers between them; the trailing
    // `Min(8)` lets the PM row absorb any extra terminal height.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(13),
            Constraint::Length(1),
            Constraint::Length(7),
            Constraint::Length(1),
            Constraint::Min(8),
        ])
        .split(area);

    let metrics = app.metrics();

    render_server_url_line(frame, rows[0], app);
    let aqi_history = app.metric_history(metrics.aqi().key);
    render_air_monitor_hero_row(frame, rows[2], app, metrics.aqi(), &metrics, &aqi_history);
    render_air_monitor_gas_row(frame, rows[4], theme, &metrics);
    render_air_monitor_pm_row(frame, rows[6], theme, &metrics);
}

fn render_showcase_backdrop(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    frame.render_widget(
        Paragraph::new("")
            .style(theme.panel_style())
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_server_url_line(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let line = Line::from(vec![
        Span::styled("Server URL: ", theme.label_style()),
        Span::styled(configured_url_text(app), theme.muted_style()),
    ]);

    frame.render_widget(
        Paragraph::new(line)
            .style(theme.panel_style())
            .alignment(Alignment::Left)
            .block(widget_block(theme).padding(Padding::horizontal(1))),
        area,
    );
}

/// AQI on the left half, with temperature and humidity stacked on the right.
fn render_air_monitor_hero_row(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    aqi: &Metric,
    metrics: &Metrics,
    aqi_history: &[Option<f64>],
) {
    let theme = app.theme;
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(1),
            Constraint::Min(40),
        ])
        .split(area);
    let comfort = Layout::default()
        .direction(Direction::Vertical)
        .spacing(vertical_gap(area))
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(columns[2]);

    render_air_monitor_aqi_card(frame, columns[0], app, aqi, aqi_history);

    if let Some(metric) = metrics.by_key("temperature_c") {
        render_air_monitor_metric_card(
            frame,
            comfort[0],
            theme,
            metric,
            CardSpec {
                title: "Temperature",
                // Empty on purpose: the card then shows the status computed
                // from the reading, like every other card in this layout.
                detail: "",
                style: theme.neutral_card_style(),
            },
        );
    }

    if let Some(metric) = metrics.by_key("humidity") {
        render_air_monitor_metric_card(
            frame,
            comfort[1],
            theme,
            metric,
            CardSpec {
                title: "Humidity",
                detail: "",
                style: theme.good_card_style(),
            },
        );
    }
}

fn render_air_monitor_gas_row(frame: &mut Frame<'_>, area: Rect, theme: Theme, metrics: &Metrics) {
    render_card_row(
        frame,
        area,
        theme,
        metrics,
        &[
            (
                "co2",
                CardSpec {
                    title: "CO₂",
                    detail: "",
                    style: theme.good_card_style(),
                },
            ),
            (
                "tvoc",
                CardSpec {
                    title: "TVOC",
                    detail: "",
                    style: theme.warning_card_style(),
                },
            ),
            (
                "nox",
                CardSpec {
                    title: "NOx",
                    detail: "",
                    style: theme.good_card_style(),
                },
            ),
        ],
    );
}

fn render_air_monitor_pm_row(frame: &mut Frame<'_>, area: Rect, theme: Theme, metrics: &Metrics) {
    render_card_row(
        frame,
        area,
        theme,
        metrics,
        &[
            (
                "pm03_count",
                CardSpec {
                    title: "PM₀․₃ Count",
                    detail: "",
                    style: theme.neutral_card_style(),
                },
            ),
            (
                "pm1",
                CardSpec {
                    title: "PM₁․₀",
                    detail: "",
                    style: theme.neutral_card_style(),
                },
            ),
            (
                "pm25",
                CardSpec {
                    title: "PM₂․₅",
                    detail: "",
                    style: theme.good_card_style(),
                },
            ),
            (
                "pm10",
                CardSpec {
                    title: "PM₁₀",
                    detail: "",
                    style: theme.warning_card_style(),
                },
            ),
        ],
    );
}

/// Lays `cards` out as equal-width columns separated by 1-cell gutters, and
/// draws each one whose metric the device actually reported. A card whose
/// metric is absent leaves its column blank rather than shifting the rest.
fn render_card_row(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: Theme,
    metrics: &Metrics,
    cards: &[(&str, CardSpec)],
) {
    // `Fill` plus `spacing`, not `Ratio` plus interleaved spacer columns:
    // ratios that already account for the whole width leave the solver no room
    // for the gutters, and the interleaving made every card's column index
    // `index * 2`. The compact grid and the settings view express it this way
    // too.
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(WIDGET_GAP)
        .constraints(vec![Constraint::Fill(1); cards.len()])
        .split(area);

    for ((key, spec), column) in cards.iter().zip(columns.iter()) {
        if let Some(metric) = metrics.by_key(key) {
            render_air_monitor_metric_card(frame, *column, theme, metric, *spec);
        }
    }
}
