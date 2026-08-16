//! The trend sparkline: a metric's recent readings drawn as a bar graph.
//!
//! Fed by `TuiApp::metric_history`, which keeps the last several dozen
//! successful readings. The dashboard already answers "what is it now" (the
//! value) and "which way is it going" (the trend arrow); this answers "and for
//! how long", which is the question the other two cannot — a CO2 reading of
//! 900 means something different on the way up from a spike that is already
//! subsiding.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Sparkline},
};

use crate::{
    sensors::{MISSING_VALUE, Metric},
    tui::theme::Theme,
};

/// Rows the graph needs before it is worth drawing: one caption, and at least
/// two for bars — a one-row sparkline can only be present or absent, which says
/// less than the trend arrow already does.
pub(super) const MIN_HISTORY_HEIGHT: u16 = 3;

/// Rows beyond which a sparkline stops gaining from the extra height. Past
/// this it is competing with the text above it for no benefit.
const MAX_HISTORY_HEIGHT: u16 = 6;

/// The rows to give a sparkline sharing a panel with `text_rows` of text.
///
/// Zero when the text's own worst case — every line wrapped — would not leave
/// enough behind. The text is what the panel is for; the graph takes what is
/// spare and nothing more.
pub(super) fn history_rows_beside_text(available: u16, text_rows: u16) -> u16 {
    let spare = available.saturating_sub(text_rows);

    if spare >= MIN_HISTORY_HEIGHT {
        spare.min(MAX_HISTORY_HEIGHT)
    } else {
        0
    }
}

/// Readings below which there is no shape to show yet. Two points is a line
/// segment, not a trend.
const MIN_READINGS: usize = 3;

/// Fractional readings survive the trip through `Sparkline`'s integer bars by
/// being scaled up first: PM2.5 values of 7.4 and 7.8 are two different bar
/// heights at this resolution and the same one without it.
const FIXED_POINT: f64 = 100.0;

/// Whether `history` has enough in it, and `area` enough room, to draw.
pub(super) fn can_render_history(area: Rect, history: &[Option<f64>]) -> bool {
    area.height >= MIN_HISTORY_HEIGHT
        && area.width >= 8
        && history.iter().filter(|value| value.is_some()).count() >= MIN_READINGS
}

/// Draws `history` as a captioned sparkline, newest reading at the right.
///
/// Bar heights are relative to the tallest reading *in this window*, not to the
/// metric's health scale. Drawing an AQI window against its full 0-500 range
/// would flatten every ordinary reading onto the floor and show nothing at all;
/// the caption carries the absolute numbers so the shape can be read without
/// the axis being misleading.
pub(super) fn render_metric_history(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: Theme,
    metric: &Metric,
    history: &[Option<f64>],
) {
    if !can_render_history(area, history) {
        return;
    }

    // The graph is one bar per cell, so only the readings that fit are drawn —
    // and it is the most recent ones that are worth keeping.
    let visible = history.len().saturating_sub(usize::from(area.width));
    let window = &history[visible..];

    let [caption_area, bars_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(2)])
        .areas(area);

    let peak = window
        .iter()
        .flatten()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let low = window
        .iter()
        .flatten()
        .copied()
        .fold(f64::INFINITY, f64::min);

    frame.render_widget(
        Paragraph::new(Line::from(caption_spans(
            theme,
            metric,
            window.len(),
            low,
            peak,
        ))),
        caption_area,
    );

    let bars: Vec<Option<u64>> = window
        .iter()
        .map(|value| value.map(|value| (value.max(0.0) * FIXED_POINT) as u64))
        .collect();

    frame.render_widget(
        Sparkline::default()
            .data(bars)
            // A flat run would otherwise be scaled against itself and drawn at
            // full height, which reads as "at maximum" rather than "steady".
            .max(((peak.max(low * 2.0)).max(1.0) * FIXED_POINT) as u64)
            .style(theme.status_style(metric.status))
            .absent_value_style(theme.muted_style())
            .absent_value_symbol(MISSING_VALUE)
            .direction(ratatui::widgets::RenderDirection::LeftToRight),
        bars_area,
    );
}

/// "Last 12 readings   4 - 47" — how many, and the range they covered, so the
/// bars can be read as values rather than only as a shape.
fn caption_spans(
    theme: Theme,
    metric: &Metric,
    count: usize,
    low: f64,
    peak: f64,
) -> Vec<Span<'static>> {
    let unit = if metric.unit.is_empty() {
        String::new()
    } else {
        format!(" {}", metric.unit)
    };

    vec![
        Span::styled(format!("Last {count} readings"), theme.label_style()),
        Span::raw("   "),
        Span::styled(format!("{}{unit}", format_bound(low)), theme.muted_style()),
        Span::styled(" - ", theme.muted_style()),
        Span::styled(format!("{}{unit}", format_bound(peak)), theme.value_style()),
    ]
}

/// One decimal only when it says something, so a caption of whole-numbered
/// readings does not read as `47.0 - 47.0`.
fn format_bound(value: f64) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn area(width: u16, height: u16) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    #[test]
    fn needs_room_and_readings_before_it_will_draw() {
        let enough = [Some(1.0), Some(2.0), Some(3.0)];

        assert!(can_render_history(area(20, 3), &enough));
        // Too short, too narrow, too few readings.
        assert!(!can_render_history(area(20, 2), &enough));
        assert!(!can_render_history(area(7, 3), &enough));
        assert!(!can_render_history(area(20, 3), &[Some(1.0), Some(2.0)]));
    }

    #[test]
    fn absent_readings_do_not_count_toward_the_minimum() {
        // Three entries but only two readings: a device that dropped a sensor
        // must not make the graph appear with a hole where its data is.
        let sparse = [Some(1.0), None, Some(3.0)];

        assert!(!can_render_history(area(20, 3), &sparse));
        assert!(can_render_history(
            area(20, 3),
            &[Some(1.0), None, Some(3.0), Some(4.0)]
        ));
    }

    #[test]
    fn a_sparkline_only_takes_rows_the_text_does_not_need() {
        // Text needing 5 of 12 rows leaves 7, capped at the useful maximum.
        assert_eq!(history_rows_beside_text(12, 5), MAX_HISTORY_HEIGHT);
        // Exactly enough to clear the minimum.
        assert_eq!(history_rows_beside_text(8, 5), 3);
        // One short of it: the text keeps everything.
        assert_eq!(history_rows_beside_text(7, 5), 0);
        assert_eq!(history_rows_beside_text(5, 5), 0);
        assert_eq!(history_rows_beside_text(3, 5), 0);
    }

    #[test]
    fn bounds_drop_the_decimal_when_it_says_nothing() {
        assert_eq!(format_bound(47.0), "47");
        assert_eq!(format_bound(47.04), "47");
        assert_eq!(format_bound(7.4), "7.4");
    }
}
