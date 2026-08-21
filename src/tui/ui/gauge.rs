//! The banded gauge: a bar whose colour changes along its length, with tick
//! marks and a scale underneath.
//!
//! The bar is not one colour filled to a point. Each *filled* cell is coloured
//! by the band that its own position on the scale falls into, so a reading well
//! into the yellow shows the green run it passed through on the way there
//! rather than turning uniformly yellow.
//!
//! Past the reading the bar is hollow and muted — one neutral colour, not the
//! upcoming bands. What is drawn is therefore where the air has got to, not a
//! preview of where it could go; the tick marks and the scale underneath are
//! what say how much headroom is left.
//!
//! Three rows when there is room for all of it, gracefully fewer when there is
//! not:
//!
//! ```text
//!  ███████████░░░░░░░░░░░░░░░░░░░░░     filled and banded up to the reading,
//!                                        hollow and muted past it
//!  ▲         ▲         ▲         ▲      ticks at the band boundaries
//!  0        50        100      500      the scale those ticks sit at
//! ```

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{sensors::Metric, tui::theme::Theme};

/// Rows the gauge uses when it has them: bar, ticks, scale.
pub(super) const FULL_HEIGHT: u16 = 3;

/// The bar on its own, with no room for ticks or the scale beneath it. What a
/// card gives its gauge when the reading and its labels have taken the rest.
pub(super) const SINGLE_ROW: u16 = 1;

/// Below this width the ticks and their labels collide, so only the bar is
/// drawn.
const MIN_TICK_WIDTH: u16 = 24;

/// Cell shades from "well inside the band" to "at its top", which gives the
/// bar texture even in a terminal with a narrow palette.
const FILLED: &str = "█";
const UNFILLED: &str = "░";

/// Draws the gauge into `area`, using as much of it as it can.
///
/// One row gets the bar alone, two adds the ticks, three adds the scale.
pub(super) fn render_band_gauge(frame: &mut Frame<'_>, area: Rect, theme: Theme, metric: &Metric) {
    if area.height == 0 || area.width < 4 {
        return;
    }

    let show_ticks = area.height >= 2 && area.width >= MIN_TICK_WIDTH;
    let show_scale = area.height >= FULL_HEIGHT && area.width >= MIN_TICK_WIDTH;

    let [bar_area, tick_area, scale_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(u16::from(show_ticks)),
            Constraint::Length(u16::from(show_scale)),
        ])
        .areas(area);

    render_bar(frame, bar_area, theme, metric);

    if show_ticks {
        render_ticks(frame, tick_area, theme, metric);
    }
    if show_scale {
        render_scale(frame, scale_area, theme, metric);
    }
}

/// The bar itself: each cell coloured by the band its position falls in,
/// solid up to the reading and hollow past it.
fn render_bar(frame: &mut Frame<'_>, area: Rect, theme: Theme, metric: &Metric) {
    let width = area.width;
    let reading = metric.value.unwrap_or(0.0).clamp(0.0, metric.scale.max());
    let filled_cells = ((reading / metric.scale.max()) * f64::from(width)).round() as u16;

    let cells: Vec<Span> = (0..width)
        .map(|cell| {
            // The value this cell stands for, so its colour comes from the same
            // classifier the reading itself went through.
            let at = (f64::from(cell) + 0.5) / f64::from(width) * metric.scale.max();
            let band = metric.scale.classify(Some(at));
            let filled = cell < filled_cells;

            Span::styled(
                if filled { FILLED } else { UNFILLED },
                if filled {
                    Style::default().fg(theme.status_color(band))
                } else {
                    // One neutral colour, deliberately not this cell's band:
                    // colouring the empty run would read as a reading that is
                    // already there. The ticks below carry the headroom.
                    theme.muted_style()
                },
            )
        })
        .collect();

    frame.render_widget(Paragraph::new(Line::from(cells)), area);
}

/// A tick under each band boundary, plus one at each end of the scale.
fn render_ticks(frame: &mut Frame<'_>, area: Rect, theme: Theme, metric: &Metric) {
    let mut row = vec![" ".to_string(); usize::from(area.width)];

    for (column, _) in tick_positions(metric, area.width) {
        row[usize::from(column)] = "▲".to_string();
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(row.concat(), theme.muted_style()))),
        area,
    );
}

/// The value each tick stands for, written under it.
fn render_scale(frame: &mut Frame<'_>, area: Rect, theme: Theme, metric: &Metric) {
    let mut row = vec![' '; usize::from(area.width)];

    for (column, value) in tick_positions(metric, area.width) {
        let label = format_scale_value(value);
        // Centred under its tick where it fits, nudged inward at the edges
        // rather than clipped.
        let start = usize::from(column)
            .saturating_sub(label.chars().count() / 2)
            .min(row.len().saturating_sub(label.chars().count()));

        for (offset, character) in label.chars().enumerate() {
            if let Some(slot) = row.get_mut(start + offset) {
                *slot = character;
            }
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            row.iter().collect::<String>(),
            theme.muted_style(),
        ))),
        area,
    );
}

/// Where each band boundary lands on a bar `width` cells wide, and the value
/// it represents.
///
/// Boundaries closer together than a label's width are dropped rather than
/// drawn on top of each other — a scale that overlaps itself is worse than one
/// that shows fewer marks.
fn tick_positions(metric: &Metric, width: u16) -> Vec<(u16, f64)> {
    if width == 0 {
        return Vec::new();
    }

    let mut ticks: Vec<(u16, f64)> = Vec::new();
    let mut push = |value: f64| {
        let column = ((value / metric.scale.max()) * f64::from(width - 1)).round() as u16;
        let column = column.min(width - 1);

        // Keep the leftmost of any cluster; labels need roughly five columns.
        if ticks
            .last()
            .is_none_or(|(previous, _)| column.saturating_sub(*previous) >= 5)
        {
            ticks.push((column, value));
        }
    };

    push(0.0);
    for &(boundary, _) in metric.scale.breakpoints() {
        if boundary > 0.0 && boundary < metric.scale.max() {
            push(boundary);
        }
    }
    push(metric.scale.max());

    ticks
}

/// Scale labels stay short: a four-figure CO2 bound reads better as "2k" than
/// as a number wider than the space between two ticks.
fn format_scale_value(value: f64) -> String {
    if value < 1000.0 {
        // Whole numbers throughout: a tick at 35.4 reads as 35, and the extra
        // digit would not survive the column budget anyway.
        return format!("{value:.0}");
    }

    let thousands = value / 1000.0;
    if (thousands - thousands.round()).abs() < 0.05 {
        format!("{thousands:.0}k")
    } else {
        format!("{thousands:.1}k")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use ratatui::style::Color;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::sensors::{Metrics, SensorSnapshot, metrics};

    fn co2_metric(ppm: f64) -> Metric {
        let snapshot = SensorSnapshot {
            co2: Some(ppm),
            ..SensorSnapshot::default()
        };
        let built: Metrics = metrics(&snapshot, None);
        built.by_key("co2").expect("co2 is a metric").clone()
    }

    /// Renders just the bar row and returns each cell as (symbol, foreground).
    fn render_bar_cells(metric: &Metric, width: u16) -> Vec<(String, Color)> {
        let backend = TestBackend::new(width, 1);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        let theme = Theme::default_theme();
        terminal
            .draw(|frame| render_bar(frame, frame.area(), theme, metric))
            .expect("bar should render");

        let buffer = terminal.backend().buffer().clone();
        (0..width)
            .map(|column| {
                let cell = &buffer[(column, 0)];
                (
                    cell.symbol().to_owned(),
                    cell.style().fg.expect("every bar cell is coloured"),
                )
            })
            .collect()
    }

    #[test]
    fn the_filled_run_is_banded_and_the_rest_is_hollow_and_muted() {
        // CO2 bands break at 800/1000/1500 on a 2000 scale, so a reading of
        // 1600 fills four fifths of the bar and that filled run crosses three
        // bands — which is the point of the banded gauge over a plain one.
        let metric = co2_metric(1600.0);
        let theme = Theme::default_theme();
        let cells = render_bar_cells(&metric, 40);

        let filled: Vec<_> = cells.iter().filter(|(symbol, _)| symbol == "█").collect();
        let hollow: Vec<_> = cells.iter().filter(|(symbol, _)| symbol == "░").collect();
        assert_eq!(
            filled.len() + hollow.len(),
            cells.len(),
            "the bar draws only these two glyphs"
        );
        assert!(!filled.is_empty() && !hollow.is_empty());

        // The banded run is real: the filled cells are not all one colour.
        let filled_colours: HashSet<_> = filled.iter().map(|(_, fg)| *fg).collect();
        assert!(
            filled_colours.len() > 1,
            "filled cells should carry their own band's colour, got {filled_colours:?}"
        );

        // Past the reading every cell is the one muted colour, deliberately
        // not the band it sits in — this is what the module header promises.
        let muted = theme.muted_style().fg.expect("muted has a colour");
        assert!(
            hollow.iter().all(|(_, fg)| *fg == muted),
            "cells past the reading must all be muted"
        );
    }

    #[test]
    fn ticks_start_at_zero_and_end_at_the_scale_maximum() {
        let metric = co2_metric(700.0);
        let ticks = tick_positions(&metric, 40);

        assert_eq!(ticks.first().map(|(column, _)| *column), Some(0));
        assert_eq!(ticks.last().map(|(column, _)| *column), Some(39));
        assert_eq!(ticks.last().map(|(_, value)| *value), Some(2000.0));
    }

    #[test]
    fn crowded_ticks_are_dropped_rather_than_drawn_over_each_other() {
        let metric = co2_metric(700.0);

        let roomy = tick_positions(&metric, 60);
        let cramped = tick_positions(&metric, 12);

        assert!(cramped.len() < roomy.len(), "{cramped:?} vs {roomy:?}");
        // Whatever survives, no two ticks share a column.
        let columns: Vec<u16> = cramped.iter().map(|(column, _)| *column).collect();
        let mut sorted = columns.clone();
        sorted.dedup();
        assert_eq!(columns, sorted);
    }

    #[test]
    fn scale_labels_shorten_thousands() {
        assert_eq!(format_scale_value(0.0), "0");
        assert_eq!(format_scale_value(54.0), "54");
        assert_eq!(format_scale_value(2000.0), "2k");
        assert_eq!(format_scale_value(1500.0), "1.5k");
    }

    #[test]
    fn a_zero_width_gauge_has_no_ticks_rather_than_panicking() {
        assert!(tick_positions(&co2_metric(700.0), 0).is_empty());
    }
}
