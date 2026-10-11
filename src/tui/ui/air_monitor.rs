//! Wide cockpit composition. The height budget reserves every telemetry row
//! before giving extra room to the AQI hero; no card is sacrificed to chrome.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
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
            components,
            format::configured_url_text,
        },
    },
};

/// Reserve the small cards first. Even the 28-row wide-layout threshold fits
/// a complete particulate row, unlike an overconstrained stack of fixed rows.
fn dashboard_rows(area: Rect) -> [Rect; 7] {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(11),
            Constraint::Length(1),
            Constraint::Length(6),
            Constraint::Length(1),
            Constraint::Length(7),
        ])
        .areas(area)
}

pub(super) fn render_advanced_dashboard(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    frame.render_widget(Paragraph::new("").style(theme.page_style()), area);
    let [
        source,
        overview,
        hero,
        gases,
        gas_cards,
        particles,
        pm_cards,
    ] = dashboard_rows(area);
    let metrics = app.metrics();

    render_server_url_line(frame, source, app);
    render_section(
        frame,
        overview,
        theme,
        "OVERVIEW",
        "Air quality / indoor comfort",
    );
    let aqi_history = app.metric_history(metrics.aqi().key);
    render_air_monitor_hero_row(frame, hero, app, metrics.aqi(), &metrics, &aqi_history);
    render_section(
        frame,
        gases,
        theme,
        "GASES",
        "Ventilation / volatile compounds",
    );
    render_card_row(
        frame,
        gas_cards,
        theme,
        &metrics,
        &[("co2", "CO₂"), ("tvoc", "TVOC"), ("nox", "NOx")],
    );
    render_section(
        frame,
        particles,
        theme,
        "PARTICULATES",
        "Particle count / mass concentration",
    );
    render_card_row(
        frame,
        pm_cards,
        theme,
        &metrics,
        &[
            ("pm03_count", "PM₀․₃ Count"),
            ("pm1", "PM₁․₀"),
            ("pm25", "PM₂․₅"),
            ("pm10", "PM₁₀"),
        ],
    );
}

fn render_section(frame: &mut Frame<'_>, area: Rect, theme: Theme, label: &str, detail: &str) {
    frame.render_widget(
        Paragraph::new(components::section(theme, label, detail)).style(theme.page_style()),
        area,
    );
}

fn render_server_url_line(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Server URL: ", theme.label_style()),
            Span::styled(configured_url_text(app), theme.muted_style()),
        ]))
        .style(theme.page_style()),
        area,
    );
}

/// AQI receives the strongest hierarchy; comfort readings stay stacked beside
/// it without paying for separate, mostly empty icon columns.
fn render_air_monitor_hero_row(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    aqi: &Metric,
    metrics: &Metrics,
    aqi_history: &[Option<f64>],
) {
    let [hero, comfort] = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(WIDGET_GAP)
        .constraints([Constraint::Fill(3), Constraint::Fill(2)])
        .areas(area);
    let comfort = Layout::default()
        .direction(Direction::Vertical)
        .spacing(WIDGET_GAP)
        .constraints([Constraint::Fill(1), Constraint::Fill(1)])
        .split(comfort);

    render_air_monitor_aqi_card(frame, hero, app, aqi, aqi_history);
    for ((key, title), area) in [("temperature_c", "Temperature"), ("humidity", "Humidity")]
        .into_iter()
        .zip(comfort.iter())
    {
        if let Some(metric) = metrics.by_key(key) {
            render_air_monitor_metric_card(frame, *area, app.theme, metric, CardSpec { title });
        }
    }
}

/// Keep every metric's column, including missing readings. A sensor dropout
/// changes its value and status, never the spatial map of the dashboard.
fn render_card_row(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: Theme,
    metrics: &Metrics,
    cards: &[(&str, &'static str)],
) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(WIDGET_GAP)
        .constraints(vec![Constraint::Fill(1); cards.len()])
        .split(area);
    for ((key, title), column) in cards.iter().zip(columns.iter()) {
        if let Some(metric) = metrics.by_key(key) {
            render_air_monitor_metric_card(frame, *column, theme, metric, CardSpec { title });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{refresh_interval::RefreshInterval, sensors::SensorSnapshot, tui::ui};
    use ratatui::{Terminal, backend::TestBackend};
    use std::time::Duration;

    #[test]
    fn wide_height_budget_keeps_every_card_at_the_layout_threshold() {
        for height in [28, 29, 32, 34, 44, 60] {
            let rows = dashboard_rows(Rect::new(1, 3, 118, height));
            assert!(rows[2].height >= 11);
            assert_eq!(rows[4].height, 6);
            assert_eq!(rows[6].height, 7);
            assert_eq!(rows[6].bottom(), 3 + height);
            assert_eq!(rows.iter().map(|row| row.height).sum::<u16>(), height);
        }
    }

    #[test]
    fn all_wide_readings_and_units_survive_a_120_by_40_frame() {
        let mut app = TuiApp::new(None, RefreshInterval::clamping(Duration::from_secs(30)));
        app.current_snapshot = Some(SensorSnapshot {
            aqi: Some(42.0),
            co2: Some(612.0),
            tvoc: Some(82.0),
            nox: Some(3.0),
            pm03_count: Some(1_000_000.0),
            pm1: Some(4.1),
            pm25: Some(7.4),
            pm10: Some(9.8),
            temperature_c: Some(22.6),
            humidity: Some(48.1),
        });
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
        terminal
            .draw(|frame| ui::draw(frame, &app))
            .expect("render");
        let buffer = terminal.backend().buffer();
        let output = (0..40)
            .map(|y| {
                (0..120)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        for reading in [
            "42",
            "612 ppm",
            "82 index",
            "3 index",
            "1000000 count/dL",
            "4.1 ug/m3",
            "7.4 ug/m3",
            "9.8 ug/m3",
            "22.6 C",
            "48.1 %",
        ] {
            assert!(output.contains(reading), "missing {reading:?}\n{output}");
        }
        for heading in ["OVERVIEW", "GASES", "PARTICULATES"] {
            assert!(output.contains(heading), "missing {heading}\n{output}");
        }
    }
}
