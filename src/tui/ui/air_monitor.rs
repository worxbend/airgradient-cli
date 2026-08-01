//! The wide "Air Monitor" showcase dashboard.
//!
//! This module owns row composition only — which metric goes where, and how
//! each band is sized. Drawing an individual card lives in
//! [`super::air_monitor_card`].

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::{
    sensors::Metric,
    tui::{
        app::TuiApp,
        theme::Theme,
        ui::{
            air_monitor_card::{
                CardSpec, render_air_monitor_aqi_card, render_air_monitor_metric_card,
            },
            format::{aqi_metric, configured_url_text, display_metrics, metric_by_key},
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

    let metrics = display_metrics(app);

    render_server_url_line(frame, rows[0], app);
    render_air_monitor_hero_row(frame, rows[2], app, aqi_metric(&metrics), &metrics);
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
            .alignment(Alignment::Left),
        area,
    );
}

/// AQI on the left half, with temperature and humidity stacked on the right.
fn render_air_monitor_hero_row(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    aqi: &Metric,
    metrics: &[Metric],
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
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(columns[2]);

    render_air_monitor_aqi_card(frame, columns[0], app, aqi);

    if let Some(metric) = metric_by_key(metrics, "temperature_c") {
        render_air_monitor_metric_card(
            frame,
            comfort[0],
            theme,
            metric,
            CardSpec {
                title: "Temperature",
                icon: "♨",
                detail: "Comfort: Warm",
                style: theme.neutral_card_style(),
            },
        );
    }

    if let Some(metric) = metric_by_key(metrics, "humidity") {
        render_air_monitor_metric_card(
            frame,
            comfort[1],
            theme,
            metric,
            CardSpec {
                title: "Humidity",
                icon: "♢",
                detail: "Comfort: Comfortable",
                style: theme.good_card_style(),
            },
        );
    }
}

fn render_air_monitor_gas_row(frame: &mut Frame<'_>, area: Rect, theme: Theme, metrics: &[Metric]) {
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
                    icon: "•••",
                    detail: "",
                    style: theme.good_card_style(),
                },
            ),
            (
                "tvoc",
                CardSpec {
                    title: "TVOC",
                    icon: "⚗",
                    detail: "",
                    style: theme.warning_card_style(),
                },
            ),
            (
                "nox",
                CardSpec {
                    title: "NOx",
                    icon: "≋",
                    detail: "",
                    style: theme.good_card_style(),
                },
            ),
        ],
    );
}

fn render_air_monitor_pm_row(frame: &mut Frame<'_>, area: Rect, theme: Theme, metrics: &[Metric]) {
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
                    icon: "✣",
                    detail: "",
                    style: theme.neutral_card_style(),
                },
            ),
            (
                "pm1",
                CardSpec {
                    title: "PM₁․₀",
                    icon: "✣",
                    detail: "",
                    style: theme.neutral_card_style(),
                },
            ),
            (
                "pm25",
                CardSpec {
                    title: "PM₂․₅",
                    icon: "✣",
                    detail: "",
                    style: theme.good_card_style(),
                },
            ),
            (
                "pm10",
                CardSpec {
                    title: "PM₁₀",
                    icon: "✣",
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
    metrics: &[Metric],
    cards: &[(&str, CardSpec)],
) {
    let column_count = cards.len() as u32;
    let mut constraints = Vec::new();
    for index in 0..cards.len() {
        if index > 0 {
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Ratio(1, column_count));
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (index, (key, spec)) in cards.iter().enumerate() {
        if let Some(metric) = metric_by_key(metrics, key) {
            render_air_monitor_metric_card(frame, columns[index * 2], theme, metric, *spec);
        }
    }
}
