//! A single showcase-dashboard card: icon column, text column, fill gauge.
//!
//! The AQI card and the ordinary metric cards share a frame and a gauge but
//! differ in their text block, so each keeps its own renderer rather than
//! threading a mode flag through one.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, LineGauge, Paragraph, Wrap},
};

use crate::{
    sensors::{Metric, Status},
    tui::{
        app::TuiApp,
        theme::Theme,
        ui::format::{aqi_message, metric_value, metric_value_with_unit},
    },
};

/// The fixed presentation of a card: everything that does not come from the
/// metric reading itself.
#[derive(Clone, Copy)]
pub(super) struct CardSpec {
    pub(super) title: &'static str,
    pub(super) icon: &'static str,
    /// Sub-label under the value. Empty falls back to the metric's status.
    pub(super) detail: &'static str,
    pub(super) style: Style,
}

pub(super) fn render_air_monitor_aqi_card(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    metric: &Metric,
) {
    let theme = app.theme;
    let block = air_monitor_card_block(theme, theme.good_card_style(), metric.status);
    let inner = pad_card_area(block.inner(area));
    frame.render_widget(block, area);

    let (icon_area, text_area, gauge_area) = split_card(inner, 11, 24);
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
        Line::from(trend_spans(theme, metric)),
    ];

    render_card_icon(frame, icon_area, theme, "◒");
    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.good_card_style())
            .wrap(Wrap { trim: true }),
        text_area,
    );
    render_metric_gauge(frame, gauge_area, theme, metric);
}

pub(super) fn render_air_monitor_metric_card(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: Theme,
    metric: &Metric,
    spec: CardSpec,
) {
    let block = air_monitor_card_block(theme, spec.style, metric.status);
    let inner = pad_card_area(block.inner(area));
    frame.render_widget(block, area);

    let (icon_area, text_area, gauge_area) = split_card(inner, 7, 16);
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
        Line::from(trend_spans(theme, metric)),
    ];

    render_card_icon(frame, icon_area, theme, spec.icon);
    frame.render_widget(
        Paragraph::new(lines)
            .style(spec.style)
            .wrap(Wrap { trim: true }),
        text_area,
    );
    render_metric_gauge(frame, gauge_area, theme, metric);
}

/// Splits a card's inner area into its icon column, text block, and the
/// single-row gauge pinned to the bottom of the text column.
fn split_card(inner: Rect, icon_width: u16, min_text_width: u16) -> (Rect, Rect, Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(icon_width),
            Constraint::Min(min_text_width),
        ])
        .split(inner);
    let text_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(columns[1]);

    (columns[0], text_rows[0], text_rows[1])
}

fn render_card_icon(frame: &mut Frame<'_>, area: Rect, theme: Theme, icon: &str) {
    frame.render_widget(
        Paragraph::new(Span::styled(
            icon.to_string(),
            theme.value_style().add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        area,
    );
}

fn trend_spans(theme: Theme, metric: &Metric) -> Vec<Span<'static>> {
    vec![
        Span::styled(metric.trend.display_symbol(), theme.value_style()),
        Span::raw(" from last reading"),
    ]
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
        .map(|value| (value / metric.scale_max).clamp(0.0, 1.0))
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

/// A card's frame: a full rounded border tinted by the reading's status.
///
/// The border used to be a single thick bar down the left edge. That left the
/// card open on three sides, so where one card ended and the next began was
/// carried entirely by the gap between them — and a card at the edge of a row
/// had no visible extent at all. A closed border states it outright, and
/// keeping it status-coloured preserves the at-a-glance signal the left bar
/// carried.
fn air_monitor_card_block(theme: Theme, style: Style, status: Status) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_set(symbols::border::ROUNDED)
        .border_style(theme.status_style(status))
        .style(style)
}

/// Insets a card's contents by one column on each side.
///
/// The border already supplies the vertical inset that this used to add by
/// hand; what it cannot supply is horizontal breathing room, so text does not
/// start immediately against the left border. Cards too narrow to give up two
/// columns keep them.
fn pad_card_area(area: Rect) -> Rect {
    let horizontal = if area.width >= 12 { 1 } else { 0 };
    Rect {
        x: area.x.saturating_add(horizontal),
        y: area.y,
        width: area.width.saturating_sub(horizontal * 2),
        height: area.height,
    }
}
