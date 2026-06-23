use std::time::{Duration, SystemTime};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    sensors::{Metric, SensorSnapshot, Status, Trend, metrics},
    tui::{app::TuiApp, theme},
};

const ADVANCED_LAYOUT_WIDTH: u16 = 112;
const ADVANCED_LAYOUT_HEIGHT: u16 = 28;

pub fn draw(frame: &mut Frame<'_>, app: &TuiApp) {
    let area = frame.area();
    if area.width < theme::MIN_TERMINAL_WIDTH || area.height < theme::MIN_TERMINAL_HEIGHT {
        render_terminal_too_small(frame, area);
        return;
    }

    let has_error = app.current_error.is_some();
    let error_height = if has_error { 5 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(error_height),
            Constraint::Length(3),
        ])
        .split(area);

    render_top_bar(frame, chunks[0], app);
    render_dashboard(frame, chunks[1], app);

    if has_error {
        render_error(frame, chunks[2], app);
    }

    render_footer(frame, chunks[3]);
}

fn render_terminal_too_small(frame: &mut Frame<'_>, area: Rect) {
    let lines = vec![
        Line::from(Span::styled("Terminal too small", theme::error_style())),
        Line::from(Span::styled(
            format!(
                "Minimum {}x{}",
                theme::MIN_TERMINAL_WIDTH,
                theme::MIN_TERMINAL_HEIGHT
            ),
            theme::muted_style(),
        )),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn render_top_bar(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    if area.width >= ADVANCED_LAYOUT_WIDTH {
        render_showcase_top_bar(frame, area, app);
        return;
    }

    let url = app
        .configured_url
        .as_ref()
        .map(|url| url.as_str().trim_end_matches('/').to_string())
        .unwrap_or_else(|| "no device URL configured".to_string());
    let refresh = format_duration(app.refresh_interval);
    let status = fetch_status(app);

    let line = Line::from(vec![
        Span::styled("AirGradient", theme::title_style()),
        Span::raw("  "),
        Span::styled(url, theme::muted_style()),
        Span::raw("  |  refresh "),
        Span::styled(refresh, theme::value_style()),
        Span::raw("  |  "),
        status,
    ]);

    frame.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn render_showcase_top_bar(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let updated = app
        .last_success_at
        .map(format_system_time_delta)
        .unwrap_or_else(|| "not yet".to_string());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(symbols::border::ROUNDED)
        .border_style(theme::border_style())
        .style(theme::panel_style());
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
            Span::styled("×", theme::muted_style()),
            Span::raw("   "),
            Span::styled("⌂", theme::value_style()),
            Span::raw("   "),
            Span::styled("↻", theme::value_style()),
            Span::raw("   "),
            Span::styled("Last updated: ", theme::label_style()),
            Span::styled(updated, theme::value_style()),
        ]))
        .alignment(Alignment::Left),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(Span::styled("Air Monitor", theme::value_style()))
            .alignment(Alignment::Center),
        chunks[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("⚙", theme::value_style()),
            Span::raw("   "),
            Span::styled("ⓘ", theme::value_style()),
        ]))
        .alignment(Alignment::Right),
        chunks[2],
    );
}

fn render_dashboard(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    if area.width >= ADVANCED_LAYOUT_WIDTH && area.height >= ADVANCED_LAYOUT_HEIGHT {
        render_advanced_dashboard(frame, area, app);
        return;
    }

    render_compact_dashboard(frame, area, app);
}

fn render_advanced_dashboard(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    render_showcase_backdrop(frame, area);

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
    let aqi = metrics
        .iter()
        .find(|metric| metric.key == "aqi")
        .expect("presentation metrics always include AQI");

    render_server_url_line(frame, rows[0], app);
    render_air_monitor_hero_row(frame, rows[2], app, aqi, &metrics);
    render_air_monitor_gas_row(frame, rows[4], &metrics);
    render_air_monitor_pm_row(frame, rows[6], &metrics);
}

fn render_showcase_backdrop(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new("")
            .style(theme::panel_style())
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_server_url_line(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let url = app
        .configured_url
        .as_ref()
        .map(|url| url.as_str().trim_end_matches('/').to_string())
        .unwrap_or_else(|| "no device URL configured".to_string());
    let line = Line::from(vec![
        Span::styled("Server URL: ", theme::label_style()),
        Span::styled(url, theme::muted_style()),
    ]);

    frame.render_widget(
        Paragraph::new(line)
            .style(theme::panel_style())
            .alignment(Alignment::Left),
        area,
    );
}

fn render_air_monitor_hero_row(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    aqi: &Metric,
    metrics: &[Metric],
) {
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
            metric,
            CardSpec {
                title: "Temperature",
                icon: "♨",
                detail: "Comfort: Warm",
                style: theme::neutral_card_style(),
            },
        );
    }

    if let Some(metric) = metric_by_key(metrics, "humidity") {
        render_air_monitor_metric_card(
            frame,
            comfort[1],
            metric,
            CardSpec {
                title: "Humidity",
                icon: "♢",
                detail: "Comfort: Comfortable",
                style: theme::good_card_style(),
            },
        );
    }
}

fn render_air_monitor_gas_row(frame: &mut Frame<'_>, area: Rect, metrics: &[Metric]) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Length(1),
            Constraint::Ratio(1, 3),
            Constraint::Length(1),
            Constraint::Ratio(1, 3),
        ])
        .split(area);
    let cards = [
        (
            "co2",
            CardSpec {
                title: "CO₂",
                icon: "•••",
                detail: "",
                style: theme::good_card_style(),
            },
        ),
        (
            "tvoc",
            CardSpec {
                title: "TVOC",
                icon: "⚗",
                detail: "",
                style: theme::warning_card_style(),
            },
        ),
        (
            "nox",
            CardSpec {
                title: "NOx",
                icon: "≋",
                detail: "",
                style: theme::good_card_style(),
            },
        ),
    ];

    for ((key, spec), area) in cards.iter().zip([columns[0], columns[2], columns[4]]) {
        if let Some(metric) = metric_by_key(metrics, key) {
            render_air_monitor_metric_card(frame, area, metric, *spec);
        }
    }
}

fn render_air_monitor_pm_row(frame: &mut Frame<'_>, area: Rect, metrics: &[Metric]) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Length(1),
            Constraint::Ratio(1, 4),
            Constraint::Length(1),
            Constraint::Ratio(1, 4),
            Constraint::Length(1),
            Constraint::Ratio(1, 4),
        ])
        .split(area);
    let cards = [
        (
            "pm03_count",
            CardSpec {
                title: "PM₀․₃ Count",
                icon: "✣",
                detail: "",
                style: theme::neutral_card_style(),
            },
        ),
        (
            "pm1",
            CardSpec {
                title: "PM₁․₀",
                icon: "✣",
                detail: "",
                style: theme::neutral_card_style(),
            },
        ),
        (
            "pm25",
            CardSpec {
                title: "PM₂․₅",
                icon: "✣",
                detail: "",
                style: theme::good_card_style(),
            },
        ),
        (
            "pm10",
            CardSpec {
                title: "PM₁₀",
                icon: "✣",
                detail: "",
                style: theme::warning_card_style(),
            },
        ),
    ];

    for ((key, spec), area) in cards
        .iter()
        .zip([columns[0], columns[2], columns[4], columns[6]])
    {
        if let Some(metric) = metric_by_key(metrics, key) {
            render_air_monitor_metric_card(frame, area, metric, *spec);
        }
    }
}

fn render_air_monitor_aqi_card(frame: &mut Frame<'_>, area: Rect, app: &TuiApp, metric: &Metric) {
    let block = air_monitor_card_block(theme::good_card_style(), metric.status);
    let inner = pad_card_area(block.inner(area));
    frame.render_widget(block, area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(11), Constraint::Min(24)])
        .split(inner);
    let lines = vec![
        Line::from(Span::styled("Air Quality Index", theme::value_style())),
        Line::from(vec![
            Span::styled(
                metric_value(metric),
                theme::value_style().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(metric.status.label(), theme::value_style()),
        ]),
        Line::from(Span::styled(aqi_message(app), theme::label_style())),
        Line::from(vec![
            Span::styled(render_trend(metric.trend), theme::value_style()),
            Span::raw(" from last reading"),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(Span::styled(
            "◒",
            theme::value_style().add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(lines)
            .style(theme::good_card_style())
            .wrap(Wrap { trim: true }),
        columns[1],
    );
}

fn render_air_monitor_metric_card(
    frame: &mut Frame<'_>,
    area: Rect,
    metric: &Metric,
    spec: CardSpec,
) {
    let block = air_monitor_card_block(spec.style, metric.status);
    let inner = pad_card_area(block.inner(area));
    frame.render_widget(block, area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(7), Constraint::Min(16)])
        .split(inner);
    let detail = if spec.detail.is_empty() {
        metric.status.label()
    } else {
        spec.detail
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(spec.title, theme::value_style()),
            Span::raw(" "),
            Span::styled("●", theme::status_style(metric.status)),
        ]),
        Line::from(Span::styled(
            metric_value_with_unit(metric),
            theme::value_style().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(detail, theme::label_style())),
        Line::from(vec![
            Span::styled(render_trend(metric.trend), theme::value_style()),
            Span::raw(" from last reading"),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(Span::styled(
            spec.icon,
            theme::value_style().add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(lines)
            .style(spec.style)
            .wrap(Wrap { trim: true }),
        columns[1],
    );
}

fn air_monitor_card_block(style: ratatui::style::Style, status: Status) -> Block<'static> {
    Block::default()
        .borders(Borders::LEFT)
        .border_set(symbols::border::THICK)
        .border_style(theme::status_style(status))
        .style(style)
}

fn pad_card_area(area: Rect) -> Rect {
    let vertical = if area.height >= 6 { 1 } else { 0 };
    Rect {
        x: area.x,
        y: area.y.saturating_add(vertical),
        width: area.width,
        height: area.height.saturating_sub(vertical * 2),
    }
}

fn render_compact_dashboard(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let metrics = display_metrics(app);
    let aqi = metrics
        .iter()
        .find(|metric| metric.key == "aqi")
        .expect("presentation metrics always include AQI");
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(3)])
        .split(area);

    render_aqi(frame, chunks[0], aqi, app);
    render_metric_grid(frame, chunks[1], &metrics);
}

fn render_aqi(frame: &mut Frame<'_>, area: Rect, metric: &Metric, app: &TuiApp) {
    let value = metric_value(metric);
    let status = metric.status.label();

    let lines = vec![
        Line::from(vec![
            Span::styled("AQI ", theme::label_style()),
            Span::styled(
                value,
                theme::status_style(metric.status).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(status, theme::status_style(metric.status)),
        ]),
        Line::from(Span::styled(aqi_message(app), theme::muted_style())),
        Line::from(vec![
            Span::styled("Trend ", theme::label_style()),
            Span::styled(render_trend(metric.trend), theme::trend_style(metric.trend)),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled("Air Quality", theme::title_style())),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_metric_grid(frame: &mut Frame<'_>, area: Rect, metrics: &[Metric]) {
    // The minimum 36x20 dashboard keeps the surrounding panels and controls
    // coherent; lower-priority metric rows may be clipped by this area.
    let column_count = if area.width >= 72 { 2 } else { 1 };
    let row_count = metrics.len().div_ceil(column_count);
    let row_constraints = vec![Constraint::Length(4); row_count];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    for (row_index, row_area) in rows.iter().enumerate() {
        let row_metrics = metrics
            .iter()
            .skip(row_index * column_count)
            .take(column_count)
            .collect::<Vec<_>>();
        if row_metrics.is_empty() {
            continue;
        }

        let column_constraints =
            vec![Constraint::Percentage(100 / column_count as u16); column_count];
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(column_constraints)
            .split(*row_area);

        for (column_area, metric) in columns.iter().zip(row_metrics) {
            render_metric_cell(frame, *column_area, metric);
        }
    }
}

fn render_metric_cell(frame: &mut Frame<'_>, area: Rect, metric: &Metric) {
    let value = metric_value(metric);
    let unit = if metric.unit.is_empty() {
        String::new()
    } else {
        format!(" {}", metric.unit)
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(metric.label, theme::label_style()),
            Span::raw("  "),
            Span::styled(metric.status.label(), theme::status_style(metric.status)),
        ]),
        Line::from(vec![
            Span::styled(value, theme::value_style()),
            Span::styled(unit, theme::muted_style()),
            Span::raw("  "),
            Span::styled(render_trend(metric.trend), theme::trend_style(metric.trend)),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn render_error(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let message = app.current_error.as_deref().unwrap_or("unknown error");
    let title = if app.is_fetching {
        "Retrying After Error"
    } else {
        "Current Error"
    };
    let paragraph = if app.is_fetching {
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Retrying now; previous error:",
                theme::error_style(),
            )),
            Line::from(Span::styled(message, theme::error_style())),
        ])
    } else {
        Paragraph::new(message).style(theme::error_style())
    };

    frame.render_widget(
        paragraph
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled(title, theme::error_style())),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect) {
    if area.width >= ADVANCED_LAYOUT_WIDTH {
        render_showcase_footer(frame, area);
        return;
    }

    let mut spans = vec![
        Span::styled("r", theme::value_style()),
        Span::raw(" refresh   "),
    ];

    if area.width >= 56 {
        spans.extend([
            Span::styled("+/-", theme::value_style()),
            Span::raw(" interval   "),
        ]);
    }

    spans.extend([
        Span::styled("q", theme::value_style()),
        Span::raw(" quit   "),
        Span::styled("Esc", theme::value_style()),
        Span::raw(" quit"),
    ]);

    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(theme::muted_style())
            .block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn render_showcase_footer(frame: &mut Frame<'_>, area: Rect) {
    let line = Line::from(Span::styled(
        "Latest measurements loaded.   r refresh   +/- interval   q/Esc quit",
        theme::label_style(),
    ));

    frame.render_widget(
        Paragraph::new(line)
            .style(theme::footer_style())
            .alignment(Alignment::Left),
        area,
    );
}

fn display_metrics(app: &TuiApp) -> Vec<Metric> {
    let app_metrics = app.metrics();
    if app_metrics.is_empty() {
        metrics(&SensorSnapshot::default(), None)
    } else {
        app_metrics
    }
}

#[derive(Clone, Copy)]
struct CardSpec {
    title: &'static str,
    icon: &'static str,
    detail: &'static str,
    style: ratatui::style::Style,
}

fn metric_by_key<'a>(metrics: &'a [Metric], key: &str) -> Option<&'a Metric> {
    metrics.iter().find(|metric| metric.key == key)
}

fn metric_value(metric: &Metric) -> &str {
    metric
        .formatted_value
        .as_deref()
        .unwrap_or(theme::MISSING_VALUE)
}

fn metric_value_with_unit(metric: &Metric) -> String {
    if metric.unit.is_empty() {
        metric_value(metric).to_string()
    } else {
        format!("{} {}", metric_value(metric), metric.unit)
    }
}

fn aqi_message(app: &TuiApp) -> &'static str {
    if app.configured_url.is_none() {
        "Set a device URL with config set-url or pass a URL override."
    } else if app.is_fetching && app.current_error.is_some() && app.current_snapshot.is_none() {
        "Retrying after a fetch failure; waiting for first successful reading."
    } else if app.is_fetching && app.current_error.is_some() {
        "Retrying after a fetch failure; showing the latest successful reading."
    } else if app.is_fetching && app.current_snapshot.is_none() {
        "Fetching the first air quality reading."
    } else if app.is_fetching {
        "Refreshing; showing the latest successful reading."
    } else if app.current_snapshot.is_none() {
        "Waiting for the first successful reading."
    } else {
        "Latest air quality reading."
    }
}

fn fetch_status(app: &TuiApp) -> Span<'static> {
    if app.configured_url.is_none() {
        return Span::styled("missing config", theme::error_style());
    }

    if app.is_fetching {
        if app.current_error.is_some() {
            return Span::styled("retrying", theme::error_style());
        }

        if app.current_snapshot.is_some() {
            return Span::styled("refreshing", theme::muted_style());
        }

        return Span::styled("fetching", theme::muted_style());
    }

    if app.current_error.is_some() {
        return Span::styled("fetch failed", theme::error_style());
    }

    if let Some(last_success_at) = app.last_success_at {
        let mut text = format!("updated {}", format_system_time_delta(last_success_at));
        if let Some(fetch_duration) = app.last_fetch_duration {
            text.push_str(&format!(" in {}", format_duration(fetch_duration)));
        }
        return Span::styled(text, theme::muted_style());
    }

    Span::styled("waiting for first fetch", theme::muted_style())
}

fn render_trend(trend: Trend) -> &'static str {
    match trend {
        Trend::Unknown => theme::MISSING_VALUE,
        _ => trend.symbol(),
    }
}

fn format_system_time_delta(time: SystemTime) -> String {
    match SystemTime::now().duration_since(time) {
        Ok(elapsed) => format!("{} ago", format_duration(elapsed)),
        Err(_) => "just now".to_string(),
    }
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds == 0 {
        return format!("{}ms", duration.as_millis());
    }

    if seconds < 60 {
        return format!("{seconds}s");
    }

    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    if minutes < 60 {
        if remaining_seconds == 0 {
            format!("{minutes}m")
        } else {
            format!("{minutes}m {remaining_seconds}s")
        }
    } else {
        let hours = minutes / 60;
        let remaining_minutes = minutes % 60;
        if remaining_minutes == 0 {
            format!("{hours}h")
        } else {
            format!("{hours}h {remaining_minutes}m")
        }
    }
}
