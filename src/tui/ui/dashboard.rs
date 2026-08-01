//! The dashboard body: picks the wide or compact layout, and owns the
//! compact one.
//!
//! The compact layout is the fallback for any terminal too small for the
//! showcase in [`super::air_monitor`]. It is deliberately plain — a headline
//! AQI panel over a bordered metric grid — because it has to stay readable
//! down to the 36x20 minimum.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    sensors::Metric,
    tui::{
        app::TuiApp,
        theme::Theme,
        ui::{
            ADVANCED_LAYOUT_HEIGHT, ADVANCED_LAYOUT_WIDTH, air_monitor,
            format::{aqi_message, aqi_metric, display_metrics, metric_value},
        },
    },
};

/// Width at or above which the compact grid uses two columns instead of one.
const TWO_COLUMN_GRID_WIDTH: u16 = 72;

pub(super) fn render_dashboard(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    if area.width >= ADVANCED_LAYOUT_WIDTH && area.height >= ADVANCED_LAYOUT_HEIGHT {
        air_monitor::render_advanced_dashboard(frame, area, app);
        return;
    }

    render_compact_dashboard(frame, area, app);
}

fn render_compact_dashboard(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let metrics = display_metrics(app);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(3)])
        .split(area);

    render_aqi(frame, chunks[0], aqi_metric(&metrics), app);
    render_metric_grid(frame, chunks[1], app.theme, &metrics);
}

fn render_aqi(frame: &mut Frame<'_>, area: Rect, metric: &Metric, app: &TuiApp) {
    let theme = app.theme;
    let lines = vec![
        Line::from(vec![
            Span::styled("AQI ", theme.label_style()),
            Span::styled(
                metric_value(metric),
                theme
                    .status_style(metric.status)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(metric.status.label(), theme.status_style(metric.status)),
        ]),
        Line::from(Span::styled(aqi_message(app), theme.muted_style())),
        Line::from(vec![
            Span::styled("Trend ", theme.label_style()),
            Span::styled(
                metric.trend.display_symbol(),
                theme.trend_style(metric.trend),
            ),
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
    let column_count = if area.width >= TWO_COLUMN_GRID_WIDTH {
        2
    } else {
        1
    };
    let row_count = metrics.len().div_ceil(column_count);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(4); row_count])
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
            Span::styled(metric_value(metric), theme.value_style()),
            Span::styled(unit, theme.muted_style()),
            Span::raw("  "),
            Span::styled(
                metric.trend.display_symbol(),
                theme.trend_style(metric.trend),
            ),
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
