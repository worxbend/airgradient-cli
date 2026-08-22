//! The full-screen reading chart.
//!
//! The dashboard answers "what is it now" and "which way is it going". This
//! answers "and for how long", which is the question the other two cannot: a
//! CO2 reading of 900 means something different on the way up from a spike
//! that is already subsiding.
//!
//! One metric is plotted at a time, chosen from the list down the left. Ten
//! series on one pair of axes would need ten different units on the y axis;
//! normalising them all to 0-100 would make the axis meaningless instead.

use std::time::{Duration, SystemTime};

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Axis, Chart, Dataset, GraphType, Padding, Paragraph},
};

use crate::{
    history::Reading,
    sensors::{MISSING_VALUE, Metric, Status},
    tui::{
        app::TuiApp,
        theme::Theme,
        ui::{
            format::format_duration,
            hit::{HitMap, HitTarget},
            screen_inset, widget_block, widget_gap,
        },
    },
};

/// Width the metric list takes, leaving the rest to the plot.
///
/// Sized to its contents: a status dot, a space, the longest label
/// ("PM0.3 count", 11), and six columns for a value wide enough for a
/// five-figure particle count — plus the block's border and padding. Too
/// narrow and the values clip, which is worse than not showing them.
const METRIC_LIST_WIDTH: u16 = 24;

/// Columns the label and value get inside the list.
const METRIC_LABEL_WIDTH: usize = 12;
const METRIC_VALUE_WIDTH: usize = 6;

/// Readings below which there is no line to draw. Two points make a segment,
/// not a trend.
const MIN_PLOTTABLE_READINGS: usize = 2;

/// Fraction of the value range left as headroom above and below the line, so
/// it never runs along the frame.
const AXIS_PADDING: f64 = 0.08;

pub(super) fn render_history(frame: &mut Frame<'_>, area: Rect, app: &TuiApp, hits: &mut HitMap) {
    let theme = app.theme;
    let area = screen_inset(area);
    let metric = app.history_metric();

    let outer = widget_block(theme)
        .padding(Padding::horizontal(1))
        .border_style(theme.active_border_style())
        .title(Span::styled(
            " History — j/k metric \u{00b7} gg/G ends \u{00b7} h/Esc close ",
            theme.title_style(),
        ));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    // On a narrow terminal the plot is worth more than the list: the title bar
    // already names the metric, and j/k still change it.
    if inner.width < METRIC_LIST_WIDTH * 2 {
        render_plot(frame, inner, app, &metric);
        return;
    }

    let [list_area, plot_area] = Layout::default()
        .direction(Direction::Horizontal)
        .spacing(widget_gap(inner))
        .constraints([
            Constraint::Length(METRIC_LIST_WIDTH),
            Constraint::Min(METRIC_LIST_WIDTH),
        ])
        .areas(inner);

    render_metric_list(frame, list_area, app, hits);
    render_plot(frame, plot_area, app, &metric);
}

/// The metric picker: every metric, its live value, and a dot in its own
/// status colour so the list doubles as an at-a-glance summary.
fn render_metric_list(frame: &mut Frame<'_>, area: Rect, app: &TuiApp, hits: &mut HitMap) {
    let theme = app.theme;
    let metrics = app.metrics();
    let selected = app.history_cursor.index(metrics.len());

    let rows: Vec<Line> = metrics
        .iter()
        .enumerate()
        .map(|(index, metric)| {
            let row_style = if index == selected {
                Style::default()
                    .bg(theme.highlight_bg)
                    .fg(theme.highlight_fg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Line::from(vec![
                Span::styled(theme.icon(metric.icon), theme.status_style(metric.status)),
                Span::raw(" "),
                Span::styled(format!("{:<METRIC_LABEL_WIDTH$}", metric.label), row_style),
                Span::styled(
                    format!(
                        "{:>METRIC_VALUE_WIDTH$}",
                        metric.formatted_value.as_deref().unwrap_or(MISSING_VALUE)
                    ),
                    if index == selected {
                        row_style
                    } else {
                        theme.muted_style()
                    },
                ),
            ])
            .style(row_style)
        })
        .collect();

    let block = widget_block(theme)
        .padding(Padding::horizontal(1))
        .title(Span::styled(" Metric ", theme.title_style()));
    let list_inner = block.inner(area);
    frame.render_widget(
        Paragraph::new(rows).style(theme.panel_style()).block(block),
        area,
    );

    hits.push_rows(list_inner, metrics.len(), HitTarget::HistoryRow);
}

fn render_plot(frame: &mut Frame<'_>, area: Rect, app: &TuiApp, metric: &Metric) {
    let theme = app.theme;
    let block = widget_block(theme)
        .padding(Padding::horizontal(1))
        .border_style(theme.status_style(metric.status))
        .title(Span::styled(
            format!(" {} ", plot_title(metric)),
            theme.title_style(),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let points = plot_points(app, metric);
    if points.len() < MIN_PLOTTABLE_READINGS {
        render_waiting_for_readings(frame, inner, theme, points.len());
        return;
    }

    let Some(bounds) = PlotBounds::of(&points) else {
        return;
    };

    // Band the line by value the way btop and bottom colour their meters: the
    // points are split into one dataset per status band, so a run that climbs
    // out of "Good" changes colour where it crosses rather than being one flat
    // colour chosen from the latest reading. Splitting keeps a shared point at
    // each boundary so the segments join instead of showing a gap.
    let bands = split_into_bands(&points, metric);
    let datasets: Vec<Dataset<'_>> = bands
        .iter()
        .map(|(status, band_points)| {
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(theme.status_style(*status))
                .data(band_points)
        })
        .collect();

    frame.render_widget(
        Chart::new(datasets)
            .style(theme.panel_style())
            .x_axis(
                Axis::default()
                    .style(theme.border_style())
                    .bounds([bounds.x_min, bounds.x_max])
                    .labels(bounds.x_labels(theme)),
            )
            .y_axis(
                Axis::default()
                    .style(theme.border_style())
                    .bounds([bounds.y_min, bounds.y_max])
                    .labels(bounds.y_labels(theme, metric)),
            )
            // The single dataset is already named in the title; a legend box
            // over the top-right corner would only cover the line.
            .legend_position(None),
        inner,
    );
}

/// Splits a run of points into consecutive segments of one status band each.
///
/// Each segment repeats the point that ends the previous one, so the coloured
/// lines meet rather than leaving a one-point hole where the band changes.
fn split_into_bands(points: &[(f64, f64)], metric: &Metric) -> Vec<(Status, Vec<(f64, f64)>)> {
    let mut bands: Vec<(Status, Vec<(f64, f64)>)> = Vec::new();

    for &point in points {
        let status = metric.scale.classify(Some(point.1));

        match bands.last_mut() {
            Some((current, segment)) if *current == status => segment.push(point),
            Some((_, previous)) => {
                // Start the new band at the previous point so the two segments
                // share an edge.
                let joint = previous.last().copied();
                let mut segment = Vec::new();
                segment.extend(joint);
                segment.push(point);
                bands.push((status, segment));
            }
            None => bands.push((status, vec![point])),
        }
    }

    bands
}

/// "AQI — last 2h 15m" / "CO2 (ppm) — last 40m".
fn plot_title(metric: &Metric) -> String {
    if metric.unit.is_empty() {
        metric.label.to_string()
    } else {
        format!("{} ({})", metric.label, metric.unit)
    }
}

/// The recorded readings as `(seconds ago, value)` points, oldest first.
///
/// x is negative seconds from now, so the newest reading sits at the right-hand
/// edge at 0 and the axis reads as "how long ago". Readings the device did not
/// report are dropped rather than plotted as zero — the line then joins across
/// the gap, which is honest about there being no measurement rather than
/// inventing a plunge to the floor.
fn plot_points(app: &TuiApp, metric: &Metric) -> Vec<(f64, f64)> {
    let now = SystemTime::now();
    let values = app.metric_history(metric.key);

    app.reading_history()
        .zip(values)
        .filter_map(|(reading, value)| Some((seconds_ago(now, reading), value?)))
        .collect()
}

fn seconds_ago(now: SystemTime, reading: &Reading) -> f64 {
    // A reading stamped in the future (clock skew, or one taken between
    // frames) reads as "now" rather than as a point off the right of the axis.
    now.duration_since(reading.at())
        .map(|elapsed| -elapsed.as_secs_f64())
        .unwrap_or(0.0)
}

/// The axis ranges for a set of points.
struct PlotBounds {
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
}

impl PlotBounds {
    fn of(points: &[(f64, f64)]) -> Option<Self> {
        let first = points.first()?;
        let (mut x_min, mut x_max) = (first.0, first.0);
        let (mut y_min, mut y_max) = (first.1, first.1);

        for (x, y) in points {
            x_min = x_min.min(*x);
            x_max = x_max.max(*x);
            y_min = y_min.min(*y);
            y_max = y_max.max(*y);
        }

        // A flat reading has no range of its own, so one is invented around it
        // — otherwise the line would sit exactly on an axis bound, and a zero
        // span would make every point land in the same cell.
        let span = y_max - y_min;
        let padding = if span.abs() < f64::EPSILON {
            (y_max.abs() * 0.1).max(1.0)
        } else {
            span * AXIS_PADDING
        };

        Some(Self {
            x_min,
            // The newest point sits at 0; a chart of one instant still needs a
            // non-zero width to render into.
            x_max: if x_max - x_min < 1.0 {
                x_min + 1.0
            } else {
                x_max
            },
            // Readings cannot be negative, so the floor stops at zero rather
            // than implying values below it.
            y_min: (y_min - padding).max(0.0),
            y_max: y_max + padding,
        })
    }

    /// Oldest and newest, as ages: "2h 15m ago" … "now".
    fn x_labels(&self, theme: Theme) -> Vec<Span<'static>> {
        let oldest = Duration::from_secs_f64(self.x_min.abs());
        let midpoint = Duration::from_secs_f64((self.x_min.abs()) / 2.0);

        vec![
            Span::styled(
                format!("{} ago", format_duration(oldest)),
                theme.muted_style(),
            ),
            Span::styled(
                format!("{} ago", format_duration(midpoint)),
                theme.muted_style(),
            ),
            Span::styled("now".to_string(), theme.value_style()),
        ]
    }

    /// Low, middle, and high of the plotted range.
    fn y_labels(&self, theme: Theme, metric: &Metric) -> Vec<Span<'static>> {
        let middle = (self.y_min + self.y_max) / 2.0;

        vec![
            Span::styled(metric.format_reading(self.y_min), theme.muted_style()),
            Span::styled(metric.format_reading(middle), theme.muted_style()),
            Span::styled(metric.format_reading(self.y_max), theme.value_style()),
        ]
    }
}

/// What the plot shows before there is anything to plot.
///
/// The chart is fed by a file that previous sessions wrote, so this is only the
/// genuine first run (or a metric this device does not report) rather than
/// something the user sees every launch.
fn render_waiting_for_readings(frame: &mut Frame<'_>, area: Rect, theme: Theme, recorded: usize) {
    let message = if recorded == 0 {
        "No readings recorded for this metric yet.".to_string()
    } else {
        format!("Only {recorded} reading recorded so far; a line needs two.")
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(message, theme.muted_style())),
            Line::raw(""),
            Line::from(Span::styled(
                "Readings are recorded as they arrive, and kept between sessions.",
                theme.label_style(),
            )),
        ])
        .alignment(Alignment::Center)
        .style(theme.panel_style()),
        area,
    );
}

#[cfg(test)]
mod tests;
