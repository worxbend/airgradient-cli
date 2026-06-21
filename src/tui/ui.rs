use std::time::{Duration, SystemTime};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    sensors::{Metric, SensorSnapshot, Trend, metrics},
    tui::{app::TuiApp, theme},
};

pub fn draw(frame: &mut Frame<'_>, app: &TuiApp) {
    let area = frame.area();
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

fn render_top_bar(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
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

fn render_dashboard(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
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
    let message = if app.configured_url.is_none() {
        "Set a device URL with config set-url or pass a URL override."
    } else if app.is_fetching && app.current_snapshot.is_none() {
        "Fetching the first air quality reading."
    } else if app.is_fetching {
        "Refreshing; showing the latest successful reading."
    } else if app.current_snapshot.is_none() {
        "Waiting for the first successful reading."
    } else {
        "Latest air quality reading."
    };

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
        Line::from(Span::styled(message, theme::muted_style())),
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
    frame.render_widget(
        Paragraph::new(message)
            .style(theme::error_style())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled("Current Error", theme::error_style())),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect) {
    let line = Line::from(vec![
        Span::styled("r", theme::value_style()),
        Span::raw(" refresh   "),
        Span::styled("q", theme::value_style()),
        Span::raw(" quit   "),
        Span::styled("Esc", theme::value_style()),
        Span::raw(" quit"),
    ]);

    frame.render_widget(
        Paragraph::new(line)
            .style(theme::muted_style())
            .block(Block::default().borders(Borders::ALL)),
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

fn metric_value(metric: &Metric) -> &str {
    metric
        .formatted_value
        .as_deref()
        .unwrap_or(theme::MISSING_VALUE)
}

fn fetch_status(app: &TuiApp) -> Span<'static> {
    if app.configured_url.is_none() {
        return Span::styled("missing config", theme::error_style());
    }

    if app.is_fetching {
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
