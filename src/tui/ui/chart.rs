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
    let selected = app.history_cursor.min(metrics.len().saturating_sub(1));

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

    // One click zone per visible row, so the list can be used with a mouse the
    // way the theme picker and config editor already can.
    let visible = usize::from(list_inner.height).min(metrics.len());
    for index in 0..visible {
        hits.push(
            Rect {
                x: list_inner.x,
                y: list_inner.y.saturating_add(index as u16),
                width: list_inner.width,
                height: 1,
            },
            HitTarget::HistoryRow(index),
        );
    }
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
    let now = std::time::SystemTime::now();
    let values = app.metric_history(metric.key);

    app.reading_history()
        .zip(values)
        .filter_map(|(reading, value)| Some((seconds_ago(now, reading), value?)))
        .collect()
}

fn seconds_ago(now: std::time::SystemTime, reading: &Reading) -> f64 {
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
            Span::styled(format_axis_value(self.y_min, metric), theme.muted_style()),
            Span::styled(format_axis_value(middle, metric), theme.muted_style()),
            Span::styled(format_axis_value(self.y_max, metric), theme.value_style()),
        ]
    }
}

use std::time::Duration;

/// Axis values keep a decimal only where the metric itself does, so a CO2 axis
/// reads 400/700/1000 rather than 400.0/700.0/1000.0.
fn format_axis_value(value: f64, metric: &Metric) -> String {
    let fractional = metric
        .formatted_value
        .as_deref()
        .is_some_and(|formatted| formatted.contains('.'));

    if fractional {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
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
mod tests {
    use super::*;

    #[test]
    fn bounds_pad_the_range_without_going_below_zero() {
        let bounds = PlotBounds::of(&[(-60.0, 10.0), (-30.0, 20.0), (0.0, 15.0)])
            .expect("points should yield bounds");

        assert_eq!(bounds.x_min, -60.0);
        assert_eq!(bounds.x_max, 0.0);
        // Padded above, floored at zero below rather than going negative.
        assert!(bounds.y_max > 20.0);
        assert!(bounds.y_min >= 0.0);
        assert!(bounds.y_min < 10.0);
    }

    #[test]
    fn a_flat_series_still_gets_a_range_to_draw_in() {
        let bounds =
            PlotBounds::of(&[(-60.0, 42.0), (0.0, 42.0)]).expect("points should yield bounds");

        assert!(
            bounds.y_max > bounds.y_min,
            "a zero-height axis would put every point in one cell"
        );
        assert!(bounds.y_min < 42.0 && bounds.y_max > 42.0);
    }

    #[test]
    fn a_single_instant_still_has_a_non_zero_width() {
        let bounds = PlotBounds::of(&[(0.0, 5.0)]).expect("one point should yield bounds");

        assert!(bounds.x_max > bounds.x_min);
    }

    fn co2(ppm: f64) -> Metric {
        let snapshot = crate::sensors::SensorSnapshot {
            co2: Some(ppm),
            ..crate::sensors::SensorSnapshot::default()
        };
        crate::sensors::metrics(&snapshot, None)
            .by_key("co2")
            .expect("co2 is a metric")
            .clone()
    }

    #[test]
    fn a_line_that_stays_in_one_band_is_one_segment() {
        let metric = co2(700.0);
        let bands = split_into_bands(&[(-60.0, 600.0), (-30.0, 700.0), (0.0, 750.0)], &metric);

        assert_eq!(bands.len(), 1);
        assert_eq!(bands[0].0, Status::Good);
        assert_eq!(bands[0].1.len(), 3);
    }

    #[test]
    fn crossing_a_band_splits_the_line_and_the_halves_still_touch() {
        // 700 is Good, 1200 is Elevated.
        let metric = co2(1200.0);
        let bands = split_into_bands(&[(-60.0, 700.0), (0.0, 1200.0)], &metric);

        assert_eq!(bands.len(), 2);
        assert_eq!(bands[0].0, Status::Good);
        assert_eq!(bands[1].0, Status::Elevated);
        // The second segment starts where the first ended, so the coloured
        // lines join instead of leaving a gap.
        assert_eq!(bands[0].1.last(), bands[1].1.first());
    }

    #[test]
    fn no_points_means_no_bands_rather_than_an_empty_segment() {
        assert!(split_into_bands(&[], &co2(700.0)).is_empty());
    }

    #[test]
    fn zero_readings_have_no_bounds() {
        assert!(PlotBounds::of(&[]).is_none());
    }

    #[test]
    fn axis_values_follow_the_metrics_own_precision() {
        let whole = Metric {
            key: "co2",
            label: "CO2",
            unit: "ppm",
            value: Some(612.0),
            formatted_value: Some("612".to_string()),
            status: crate::sensors::Status::Good,
            trend: crate::sensors::Trend::Stable,
            scale: crate::sensors::thresholds::CO2,
            icon: crate::sensors::Icons {
                nerd: "",
                plain: "",
            },
        };
        let fractional = Metric {
            formatted_value: Some("7.4".to_string()),
            ..whole.clone()
        };

        assert_eq!(format_axis_value(612.4, &whole), "612");
        assert_eq!(format_axis_value(7.44, &fractional), "7.4");
    }
}
