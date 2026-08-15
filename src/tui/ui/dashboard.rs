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
    widgets::{Padding, Paragraph, Wrap},
};

use crate::{
    sensors::Metric,
    tui::{
        app::TuiApp,
        theme::Theme,
        ui::{
            ADVANCED_LAYOUT_HEIGHT, ADVANCED_LAYOUT_WIDTH, air_monitor,
            format::{aqi_message, aqi_metric, display_metrics, metric_value},
            vertical_gap, widget_block, widget_gap,
        },
    },
};

/// Width at or above which the compact grid uses two columns instead of one.
const TWO_COLUMN_GRID_WIDTH: u16 = 72;

/// A grid cell: two lines of text between a top and a bottom border.
const CELL_HEIGHT: u16 = 4;

/// A cell clipped to this height still shows both of its lines; it only loses
/// its bottom border. Anything shorter shows no reading at all.
const MIN_CELL_HEIGHT: u16 = 3;

/// How many grid rows are worth laying out in `height`, given a `gap` between
/// them.
///
/// Rows past this point are not drawn. Handing the layout more rows than fit
/// does not show more readings — each overflow row gets a single line, which
/// is exactly enough for a cell's top border and nothing else, so the grid
/// trails off into a stack of empty box lids.
fn visible_row_count(height: u16, gap: u16) -> usize {
    let mut used: u16 = 0;
    let mut count = 0;

    loop {
        let start = if count == 0 {
            0
        } else {
            used.saturating_add(gap)
        };
        let available = height.saturating_sub(start);
        if available < MIN_CELL_HEIGHT {
            return count;
        }

        used = start.saturating_add(CELL_HEIGHT.min(available));
        count += 1;
    }
}

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
        .spacing(vertical_gap(area))
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
                widget_block(theme)
                    .padding(Padding::horizontal(1))
                    .title(Span::styled(" Air Quality ", theme.title_style())),
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
    let gap = widget_gap(area);
    let needed_rows = metrics.len().div_ceil(column_count);
    // A gutter between the columns costs a column the grid can spare. A gap
    // between the rows costs a whole row of readings if it pushes the last one
    // off the bottom, so it is only taken when it costs nothing — which is
    // measured against the rows this grid actually has, not against every row
    // the height could hold.
    let fits_with = |gap| visible_row_count(area.height, gap).min(needed_rows);
    let row_gap = if fits_with(gap) == fits_with(0) {
        gap
    } else {
        0
    };
    let row_count = fits_with(row_gap);
    if row_count == 0 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .spacing(row_gap)
        .constraints(vec![Constraint::Length(CELL_HEIGHT); row_count])
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

        // `Fill` rather than `Percentage`: percentages that already add up to
        // 100 leave the layout solver no room for the gutter between them, so
        // the cards end up sharing a seam.
        let column_constraints = vec![Constraint::Fill(1); column_count];
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .spacing(gap)
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
        Paragraph::new(lines).block(widget_block(theme).padding(Padding::horizontal(1))),
        area,
    );
}
