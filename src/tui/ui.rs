use std::time::{Duration, SystemTime};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, LineGauge, Paragraph, Wrap},
};

use crate::{
    sensors::{Metric, SensorSnapshot, Status, Trend, metrics},
    tui::{
        app::{ConfigField, TuiApp, View},
        theme::{self, Theme},
    },
};

const ADVANCED_LAYOUT_WIDTH: u16 = 112;
const ADVANCED_LAYOUT_HEIGHT: u16 = 28;

/// Powerline right-pointing solid triangle. Renders as a smooth segment
/// join in a Nerd-Font/powerline-patched terminal font; degrades to a plain
/// glyph box in other fonts, which still reads fine as a separator.
const POWERLINE_SEPARATOR: &str = "\u{e0b0}";

pub fn draw(frame: &mut Frame<'_>, app: &TuiApp) {
    let area = frame.area();
    let theme = app.theme;

    // Every screen paints an opaque, theme-colored backdrop first so
    // nothing is ever terminal-transparent.
    frame.render_widget(Paragraph::new("").style(theme.panel_style()), area);

    if let Some(frame_no) = app.splash_frame {
        render_splash(frame, area, theme, frame_no);
        return;
    }

    match app.view {
        View::ThemeSettings => {
            render_theme_settings(frame, area, app);
            return;
        }
        View::ConfigEditor => {
            render_config_editor(frame, area, app);
            return;
        }
        View::CommandPalette | View::Dashboard => {}
    }

    if area.width < theme::MIN_TERMINAL_WIDTH || area.height < theme::MIN_TERMINAL_HEIGHT {
        render_terminal_too_small(frame, area, theme);
        return;
    }

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

    render_status_line(frame, chunks[0], app);
    render_dashboard(frame, chunks[1], app);

    if has_error {
        render_error(frame, chunks[2], app);
    }

    render_footer(frame, chunks[3], theme);

    if has_palette {
        render_command_palette(frame, chunks[4], app);
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

fn render_status_line(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
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
    let aqi_status = aqi_status(app);
    let aqi_bg = theme.status_color(aqi_status);

    let url = app
        .configured_url
        .as_ref()
        .map(|url| url.as_str().trim_end_matches('/').to_string())
        .unwrap_or_else(|| "no device URL configured".to_string());
    let refresh = format_duration(app.refresh_interval);
    let status = fetch_status(app);

    let line = Line::from(vec![
        Span::styled(
            " ● ",
            Style::default()
                .fg(theme.highlight_fg)
                .bg(aqi_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(POWERLINE_SEPARATOR, Style::default().fg(aqi_bg)),
        Span::raw(" "),
        Span::styled("AirGradient", theme.title_style()),
        Span::raw("  "),
        Span::styled(url, theme.muted_style()),
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
    let aqi_status = aqi_status(app);
    let aqi_bg = theme.status_color(aqi_status);
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
            Span::styled(
                " ● ",
                Style::default()
                    .fg(theme.highlight_fg)
                    .bg(aqi_bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(POWERLINE_SEPARATOR, Style::default().fg(aqi_bg)),
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

fn aqi_status(app: &TuiApp) -> Status {
    display_metrics(app)
        .iter()
        .find(|metric| metric.key == "aqi")
        .map_or(Status::Unknown, |metric| metric.status)
}

fn render_dashboard(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    if area.width >= ADVANCED_LAYOUT_WIDTH && area.height >= ADVANCED_LAYOUT_HEIGHT {
        render_advanced_dashboard(frame, area, app);
        return;
    }

    render_compact_dashboard(frame, area, app);
}

fn render_advanced_dashboard(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    render_showcase_backdrop(frame, area, theme);

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
    let url = app
        .configured_url
        .as_ref()
        .map(|url| url.as_str().trim_end_matches('/').to_string())
        .unwrap_or_else(|| "no device URL configured".to_string());
    let line = Line::from(vec![
        Span::styled("Server URL: ", theme.label_style()),
        Span::styled(url, theme.muted_style()),
    ]);

    frame.render_widget(
        Paragraph::new(line)
            .style(theme.panel_style())
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
    ];

    for ((key, spec), area) in cards.iter().zip([columns[0], columns[2], columns[4]]) {
        if let Some(metric) = metric_by_key(metrics, key) {
            render_air_monitor_metric_card(frame, area, theme, metric, *spec);
        }
    }
}

fn render_air_monitor_pm_row(frame: &mut Frame<'_>, area: Rect, theme: Theme, metrics: &[Metric]) {
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
    ];

    for ((key, spec), area) in cards
        .iter()
        .zip([columns[0], columns[2], columns[4], columns[6]])
    {
        if let Some(metric) = metric_by_key(metrics, key) {
            render_air_monitor_metric_card(frame, area, theme, metric, *spec);
        }
    }
}

fn render_air_monitor_aqi_card(frame: &mut Frame<'_>, area: Rect, app: &TuiApp, metric: &Metric) {
    let theme = app.theme;
    let block = air_monitor_card_block(theme, theme.good_card_style(), metric.status);
    let inner = pad_card_area(block.inner(area));
    frame.render_widget(block, area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(11), Constraint::Min(24)])
        .split(inner);
    let text_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(1)])
        .split(columns[1]);
    let lines = vec![
        Line::from(Span::styled("Air Quality Index", theme.value_style())),
        Line::from(vec![
            Span::styled(
                metric_value(metric),
                theme.value_style().add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(metric.status.label(), theme.value_style()),
        ]),
        Line::from(Span::styled(aqi_message(app), theme.label_style())),
        Line::from(vec![
            Span::styled(render_trend(metric.trend), theme.value_style()),
            Span::raw(" from last reading"),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(Span::styled(
            "◒",
            theme.value_style().add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.good_card_style())
            .wrap(Wrap { trim: true }),
        text_rows[0],
    );
    render_metric_gauge(frame, text_rows[1], theme, metric);
}

fn render_air_monitor_metric_card(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: Theme,
    metric: &Metric,
    spec: CardSpec,
) {
    let block = air_monitor_card_block(theme, spec.style, metric.status);
    let inner = pad_card_area(block.inner(area));
    frame.render_widget(block, area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(7), Constraint::Min(16)])
        .split(inner);
    let text_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(1)])
        .split(columns[1]);
    let detail = if spec.detail.is_empty() {
        metric.status.label()
    } else {
        spec.detail
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(spec.title, theme.value_style()),
            Span::raw(" "),
            Span::styled("●", theme.status_style(metric.status)),
        ]),
        Line::from(Span::styled(
            metric_value_with_unit(metric),
            theme.value_style().add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(detail, theme.label_style())),
        Line::from(vec![
            Span::styled(render_trend(metric.trend), theme.value_style()),
            Span::raw(" from last reading"),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(Span::styled(
            spec.icon,
            theme.value_style().add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(lines)
            .style(spec.style)
            .wrap(Wrap { trim: true }),
        text_rows[0],
    );
    render_metric_gauge(frame, text_rows[1], theme, metric);
}

/// Thin fill bar showing the metric's value against its typical scale,
/// colored by the metric's status — laid out via the card's own `Layout`
/// split so it resizes with the terminal like the rest of the dashboard.
fn render_metric_gauge(frame: &mut Frame<'_>, area: Rect, theme: Theme, metric: &Metric) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let ratio = metric
        .value
        .map(|value| (value / gauge_scale(metric.key)).clamp(0.0, 1.0))
        .unwrap_or(0.0);

    frame.render_widget(
        LineGauge::default()
            .filled_style(theme.status_style(metric.status))
            .unfilled_style(theme.muted_style())
            .line_set(symbols::line::THICK)
            .ratio(ratio),
        area,
    );
}

/// Typical upper bound for each metric's scale, used only to size the
/// gauge fill — not a health threshold (see `sensors::thresholds` for those).
fn gauge_scale(key: &str) -> f64 {
    match key {
        "aqi" => 500.0,
        "co2" => 2000.0,
        "tvoc" => 400.0,
        "nox" => 100.0,
        "pm10" => 354.0,
        "pm25" | "pm1" => 125.4,
        "pm03_count" => 5000.0,
        _ => 100.0,
    }
}

fn air_monitor_card_block(theme: Theme, style: Style, status: Status) -> Block<'static> {
    Block::default()
        .borders(Borders::LEFT)
        .border_set(symbols::border::THICK)
        .border_style(theme.status_style(status))
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
    render_metric_grid(frame, chunks[1], app.theme, &metrics);
}

fn render_aqi(frame: &mut Frame<'_>, area: Rect, metric: &Metric, app: &TuiApp) {
    let theme = app.theme;
    let value = metric_value(metric);
    let status = metric.status.label();

    let lines = vec![
        Line::from(vec![
            Span::styled("AQI ", theme.label_style()),
            Span::styled(
                value,
                theme
                    .status_style(metric.status)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(status, theme.status_style(metric.status)),
        ]),
        Line::from(Span::styled(aqi_message(app), theme.muted_style())),
        Line::from(vec![
            Span::styled("Trend ", theme.label_style()),
            Span::styled(render_trend(metric.trend), theme.trend_style(metric.trend)),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(theme.panel_style())
                    .title(Span::styled("Air Quality", theme.title_style())),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_metric_grid(frame: &mut Frame<'_>, area: Rect, theme: Theme, metrics: &[Metric]) {
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
            render_metric_cell(frame, *column_area, theme, metric);
        }
    }
}

fn render_metric_cell(frame: &mut Frame<'_>, area: Rect, theme: Theme, metric: &Metric) {
    let value = metric_value(metric);
    let unit = if metric.unit.is_empty() {
        String::new()
    } else {
        format!(" {}", metric.unit)
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(metric.label, theme.label_style()),
            Span::raw("  "),
            Span::styled(metric.status.label(), theme.status_style(metric.status)),
        ]),
        Line::from(vec![
            Span::styled(value, theme.value_style()),
            Span::styled(unit, theme.muted_style()),
            Span::raw("  "),
            Span::styled(render_trend(metric.trend), theme.trend_style(metric.trend)),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .style(theme.panel_style())
                .border_style(theme.border_style()),
        ),
        area,
    );
}

fn render_error(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
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
                theme.error_style(),
            )),
            Line::from(Span::styled(message, theme.error_style())),
        ])
    } else {
        Paragraph::new(message).style(theme.error_style())
    };

    frame.render_widget(
        paragraph
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(theme.panel_style())
                    .title(Span::styled(title, theme.error_style())),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    if area.width >= ADVANCED_LAYOUT_WIDTH {
        render_showcase_footer(frame, area, theme);
        return;
    }

    let mut spans = vec![
        Span::styled("r", theme.value_style()),
        Span::raw(" refresh   "),
    ];

    if area.width >= 56 {
        spans.extend([
            Span::styled("+/-", theme.value_style()),
            Span::raw(" interval   "),
        ]);
    }

    if area.width >= 72 {
        spans.extend([
            Span::styled(":", theme.value_style()),
            Span::raw(" palette   "),
        ]);
    }

    if area.width >= 90 {
        spans.extend([
            Span::styled("t", theme.value_style()),
            Span::raw(" theme   "),
            Span::styled("c", theme.value_style()),
            Span::raw(" config   "),
        ]);
    }

    spans.extend([
        Span::styled("q", theme.value_style()),
        Span::raw(" quit   "),
        Span::styled("Esc", theme.value_style()),
        Span::raw(" quit"),
    ]);

    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(theme.muted_style())
            .block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn render_showcase_footer(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let line = Line::from(Span::styled(
        "Latest measurements loaded.   r refresh   +/- interval   : palette   t theme   c config   q/Esc quit",
        theme.label_style(),
    ));

    frame.render_widget(
        Paragraph::new(line)
            .style(theme.footer_style())
            .alignment(Alignment::Left),
        area,
    );
}

/// Bottom overlay bar for the `:` command palette — drawn on top of the
/// live dashboard so values stay visible while typing a URL/interval.
fn render_command_palette(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let line = match &app.palette_message {
        Some((message, is_error)) => Line::from(Span::styled(
            message.clone(),
            if *is_error {
                theme.error_style()
            } else {
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD)
            },
        )),
        None => Line::from(vec![
            Span::styled(":", theme.command_style()),
            Span::raw(format!(" {}", app.palette_input)),
            Span::styled("█", theme.value_style()),
        ]),
    };

    frame.render_widget(Paragraph::new(line).style(theme.panel_style()), area);
}

/// Full-screen theme picker, styled after obsctl-rs/twi's settings view:
/// arrow keys live-preview a theme across the whole UI, Enter confirms and
/// persists it, Esc reverts to whatever was active before opening this view.
fn render_theme_settings(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;

    let outer = Block::default()
        .borders(Borders::ALL)
        .style(theme.panel_style())
        .border_style(theme.active_border_style())
        .title(" Themes — \u{2191}/\u{2193} preview \u{00b7} Enter apply \u{00b7} Esc/F2 cancel ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(inner);

    render_theme_list(frame, sections[0], app);
    render_theme_preview(frame, sections[1], theme);
}

fn render_theme_list(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let lines: Vec<Line> = theme::ALL
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let swatch = vec![
                Span::styled("██", Style::default().fg(candidate.accent)),
                Span::styled("██", Style::default().fg(candidate.success)),
                Span::styled("██", Style::default().fg(candidate.warning)),
                Span::styled("██", Style::default().fg(candidate.danger)),
                Span::raw("  "),
                Span::raw(candidate.label),
            ];
            let style = if index == app.settings_cursor {
                Style::default()
                    .bg(theme.highlight_bg)
                    .fg(theme.highlight_fg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Line::from(swatch).style(style)
        })
        .collect();

    frame.render_widget(
        Paragraph::new(lines).style(theme.panel_style()).block(
            Block::default()
                .borders(Borders::ALL)
                .style(theme.panel_style())
                .border_style(theme.border_style())
                .title(" Themes "),
        ),
        area,
    );
}

fn render_theme_preview(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .style(theme.panel_style())
        .border_style(theme.active_border_style())
        .title(format!(" Preview: {} ", theme.label));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled("AirGradient", theme.title_style())),
        Line::raw(""),
        Line::from(Span::styled(
            "● AQI 42 Good",
            Style::default().fg(theme.status_color(Status::Good)),
        )),
        Line::from(Span::styled(
            "● AQI 120 Moderate",
            Style::default().fg(theme.status_color(Status::Moderate)),
        )),
        Line::from(Span::styled(
            "● AQI 250 Unhealthy",
            Style::default().fg(theme.status_color(Status::Unhealthy)),
        )),
        Line::raw(""),
        Line::from(Span::styled("muted text", theme.muted_style())),
        Line::from(Span::styled("label text", theme.label_style())),
        Line::from(Span::styled("value text", theme.value_style())),
        Line::raw(""),
        Line::from(Span::styled(" selected row ", {
            Style::default()
                .bg(theme.highlight_bg)
                .fg(theme.highlight_fg)
        })),
    ];

    frame.render_widget(Paragraph::new(lines).style(theme.panel_style()), inner);
}

/// Full-screen config editor: navigate fields with arrows, Enter edits a
/// text field / toggles a boolean / opens the theme picker / saves, Esc
/// cancels the current field edit (or discards the whole draft and closes).
fn render_config_editor(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let outer = Block::default()
        .borders(Borders::ALL)
        .style(theme.panel_style())
        .border_style(theme.active_border_style())
        .title(
            " Config — \u{2191}/\u{2193} navigate \u{00b7} Enter edit/toggle \u{00b7} Esc cancel ",
        );
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let mut rows = Vec::new();
    for (index, field) in ConfigField::ALL.iter().enumerate() {
        let selected = index == app.config_editor_cursor;
        let editing = selected && app.config_editor_editing.is_some();
        let label = config_field_label(*field);
        let value = config_field_display_value(app, *field, editing);

        let style = if selected {
            Style::default()
                .bg(theme.highlight_bg)
                .fg(theme.highlight_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            theme.value_style()
        };

        rows.push(Line::from(vec![
            Span::styled(format!("{label:<22}"), style),
            Span::styled(value, if selected { style } else { theme.muted_style() }),
        ]));
    }

    rows.push(Line::raw(""));
    if let Some(error) = &app.config_editor_error {
        rows.push(Line::from(Span::styled(
            format!("error: {error}"),
            theme.error_style(),
        )));
    }

    frame.render_widget(
        Paragraph::new(rows)
            .style(theme.panel_style())
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn config_field_label(field: ConfigField) -> &'static str {
    match field {
        ConfigField::ServerUrl => "Server URL",
        ConfigField::RefreshInterval => "Refresh interval (s)",
        ConfigField::NotificationsEnabled => "Notifications enabled",
        ConfigField::StartMinimized => "Start minimized",
        ConfigField::Theme => "Theme",
        ConfigField::SaveAndClose => "[ Save & Close ]",
    }
}

fn config_field_display_value(app: &TuiApp, field: ConfigField, editing: bool) -> String {
    if editing {
        let buffer = app.config_editor_editing.as_deref().unwrap_or("");
        return format!("{buffer}█");
    }

    match field {
        ConfigField::ServerUrl => app
            .config_draft
            .server_url
            .clone()
            .unwrap_or_else(|| "(none)".to_string()),
        ConfigField::RefreshInterval => format!("{}s", app.config_draft.refresh_interval_secs),
        ConfigField::NotificationsEnabled => {
            bool_label(app.config_draft.notifications_enabled).to_string()
        }
        ConfigField::StartMinimized => bool_label(app.config_draft.start_minimized).to_string(),
        ConfigField::Theme => app.theme.label.to_string(),
        ConfigField::SaveAndClose => String::new(),
    }
}

fn bool_label(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

/// Startup splash: a per-letter shimmer wordmark (blending `fg` toward
/// `accent` on a traveling phase), a tagline, and a fill gauge over the
/// frame budget. Skippable by any keypress (see `runtime::run_splash`).
fn render_splash(frame: &mut Frame<'_>, area: Rect, theme: Theme, frame_no: u64) {
    const WORDMARK: &str = "AirGradient";

    let box_width = 46u16.min(area.width.saturating_sub(2)).max(10);
    let box_height = 8u16.min(area.height.saturating_sub(2)).max(5);
    let x = area.x + area.width.saturating_sub(box_width) / 2;
    let y = area.y + area.height.saturating_sub(box_height) / 2;
    let box_area = Rect {
        x,
        y,
        width: box_width,
        height: box_height,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .style(theme.panel_style())
        .border_style(theme.active_border_style());
    let inner = block.inner(box_area);
    frame.render_widget(block, box_area);

    let phase = frame_no as f64 * 0.35;
    let letters: Vec<Span> = WORDMARK
        .chars()
        .enumerate()
        .map(|(index, ch)| {
            let t = (0.5 + 0.5 * (phase + index as f64 * 0.9).sin()).clamp(0.0, 1.0) as f32;
            Span::styled(
                ch.to_string(),
                Style::default()
                    .fg(shimmer(theme.fg, theme.accent, t))
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(letters)).alignment(Alignment::Center),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(
            "Air quality, at a glance",
            theme.muted_style(),
        ))
        .alignment(Alignment::Center),
        rows[2],
    );

    let progress_area = center_horizontally(rows[3], 24);
    let ratio = (frame_no as f64 / theme::SPLASH_TOTAL_FRAMES as f64).clamp(0.0, 1.0);
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(theme.accent).bg(theme.border))
            .ratio(ratio)
            .label(""),
        progress_area,
    );
}

fn center_horizontally(area: Rect, width: u16) -> Rect {
    let width = width.min(area.width);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y,
        width,
        height: area.height,
    }
}

fn shimmer(
    base: ratatui::style::Color,
    accent: ratatui::style::Color,
    t: f32,
) -> ratatui::style::Color {
    match (base, accent) {
        (ratatui::style::Color::Rgb(br, bg, bb), ratatui::style::Color::Rgb(ar, ag, ab)) => {
            let mix = |b: u8, a: u8| -> u8 {
                let b = f32::from(b);
                let a = f32::from(a);
                (b + (a - b) * t).round().clamp(0.0, 255.0) as u8
            };
            ratatui::style::Color::Rgb(mix(br, ar), mix(bg, ag), mix(bb, ab))
        }
        _ => base,
    }
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
    let theme = app.theme;
    if app.configured_url.is_none() {
        return Span::styled("missing config", theme.error_style());
    }

    if app.is_fetching {
        if app.current_error.is_some() {
            return Span::styled("retrying", theme.error_style());
        }

        if app.current_snapshot.is_some() {
            return Span::styled("refreshing", theme.muted_style());
        }

        return Span::styled("fetching", theme.muted_style());
    }

    if app.current_error.is_some() {
        return Span::styled("fetch failed", theme.error_style());
    }

    if let Some(last_success_at) = app.last_success_at {
        let mut text = format!("updated {}", format_system_time_delta(last_success_at));
        if let Some(fetch_duration) = app.last_fetch_duration {
            text.push_str(&format!(" in {}", format_duration(fetch_duration)));
        }
        return Span::styled(text, theme.muted_style());
    }

    Span::styled("waiting for first fetch", theme.muted_style())
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
