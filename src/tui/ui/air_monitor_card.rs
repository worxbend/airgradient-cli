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
    widgets::{Block, Borders, Paragraph, Wrap},
};
use tui_slider::{Slider, symbols as slider_symbols};

use crate::{
    sensors::{MISSING_VALUE, Metric, Status},
    tui::{
        app::TuiApp,
        theme::Theme,
        ui::{
            format::{aqi_message, metric_value, metric_value_with_unit},
            gauge, history,
        },
    },
};

/// Rows the AQI card's text needs when its message wraps onto a second line:
/// heading, value, two message lines, trend.
const AQI_CARD_TEXT_ROWS_WRAPPED: u16 = 5;

/// Columns the hero card gives its icon: the glyph plus the padding that
/// centres it against five rows of text beside it.
const HERO_ICON_WIDTH: u16 = 11;

/// Columns the hero card's text needs before wrapping starts to hurt — the
/// longest line it draws is the AQI message, not the reading.
const HERO_MIN_TEXT_WIDTH: u16 = 24;

/// Columns a metric card gives its icon. Narrower than the hero card's: the
/// glyph is the same, but there are only three rows to centre it against.
const METRIC_CARD_ICON_WIDTH: u16 = 7;

/// Columns a metric card's text needs to hold its longest formatted value and
/// unit ("1013.2 hPa") without wrapping.
const METRIC_CARD_MIN_TEXT_WIDTH: u16 = 16;

/// The fixed presentation of a card: everything that does not come from the
/// metric reading itself.
#[derive(Clone, Copy)]
pub(super) struct CardSpec {
    pub(super) title: &'static str,
    pub(super) style: Style,
}

pub(super) fn render_air_monitor_aqi_card(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    metric: &Metric,
    history: &[Option<f64>],
) {
    let theme = app.theme;
    let block = air_monitor_card_block(theme, theme.good_card_style(), metric.status);
    let inner = pad_card_area(block.inner(area));
    frame.render_widget(block, area);

    let (icon_area, text_area, gauge_area) = split_card(
        inner,
        HERO_ICON_WIDTH,
        HERO_MIN_TEXT_WIDTH,
        gauge::FULL_HEIGHT,
    );
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

    // The hero card is the tallest thing on the showcase dashboard, so the
    // sparkline goes here — but only in rows the text does not need. The
    // message line wraps on a narrow card, so the text's worst case is a line
    // more than it has; `Min` then gives it everything the graph did not
    // reserve.
    let history_rows =
        history::history_rows_beside_text(text_area.height, AQI_CARD_TEXT_ROWS_WRAPPED);
    let [text_area, history_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(AQI_CARD_TEXT_ROWS_WRAPPED),
            Constraint::Length(history_rows),
        ])
        .areas(text_area);

    render_card_icon(
        frame,
        icon_area,
        theme,
        theme.icon(metric.icon),
        metric.status,
    );
    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.good_card_style())
            .wrap(Wrap { trim: true }),
        text_area,
    );
    history::render_metric_history(frame, history_area, theme, metric, history);
    // The hero card gets the full banded gauge — bar, ticks, and scale — which
    // is the one place on the dashboard with the rows to spare for it.
    gauge::render_band_gauge(frame, gauge_area, theme, metric);
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

    let (icon_area, text_area, gauge_area) = split_card(
        inner,
        METRIC_CARD_ICON_WIDTH,
        METRIC_CARD_MIN_TEXT_WIDTH,
        gauge::SINGLE_ROW,
    );
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
        Line::from(Span::styled(metric.status.label(), theme.label_style())),
        Line::from(trend_spans(theme, metric)),
    ];

    render_card_icon(
        frame,
        icon_area,
        theme,
        theme.icon(metric.icon),
        metric.status,
    );
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
fn split_card(
    inner: Rect,
    icon_width: u16,
    min_text_width: u16,
    gauge_height: u16,
) -> (Rect, Rect, Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(icon_width),
            Constraint::Min(min_text_width),
        ])
        .split(inner);
    // The gauge takes the rows it asked for only if the text still gets three;
    // a card too short for both keeps the reading and drops to a bare bar.
    let gauge_height = gauge_height.min(columns[1].height.saturating_sub(3)).max(1);
    let text_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(gauge_height)])
        .split(columns[1]);

    (columns[0], text_rows[0], text_rows[1])
}

/// The reading's own glyph, coloured by its status so the icon carries the
/// green/yellow/orange/red signal as well as the border and the bar do.
fn render_card_icon(frame: &mut Frame<'_>, area: Rect, theme: Theme, icon: &str, status: Status) {
    frame.render_widget(
        Paragraph::new(Span::styled(
            icon.to_string(),
            theme.status_style(status).add_modifier(Modifier::BOLD),
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

/// Fill bar showing the metric's value against its scale, coloured by the
/// reading's status — laid out via the card's own `Layout` split so it resizes
/// with the terminal like the rest of the dashboard.
///
/// A [`Slider`] with its handle turned off, rather than ratatui's `LineGauge`.
/// The two draw much the same bar, but the slider takes the value and its range
/// directly (`0..scale_max`) instead of a pre-computed ratio, so the scale that
/// `sensors::thresholds` owns is what reaches the widget rather than a fraction
/// this module derived from it. The handle is off because there is nothing to
/// drag: this shows a reading, it does not set one.
fn render_metric_gauge(frame: &mut Frame<'_>, area: Rect, theme: Theme, metric: &Metric) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    // A metric the device did not report reads as empty rather than as zero:
    // the bar is absent, not at the floor.
    let value = metric.value.unwrap_or(0.0).clamp(0.0, metric.scale.max());

    // How far along its scale the reading sits, kept from the gauge this
    // replaced. The slider draws its label in its own rows, and this one has
    // exactly one, so the share is rendered here instead — which also keeps it
    // in the theme's own styles rather than the widget's colours.
    let [share_area, bar_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(GAUGE_SHARE_WIDTH), Constraint::Min(4)])
        .areas(area);

    frame.render_widget(
        Paragraph::new(Span::styled(
            match metric.value {
                Some(_) => format!("{:>3.0}%", (value / metric.scale.max()) * 100.0),
                None => format!("{MISSING_VALUE:>4}"),
            },
            theme.muted_style(),
        )),
        share_area,
    );

    frame.render_widget(
        Slider::new(value, 0.0, metric.scale.max())
            .show_handle(false)
            .show_value(false)
            .filled_symbol(slider_symbols::FILLED_THICK_LINE)
            .empty_symbol(slider_symbols::FILLED_THICK_LINE)
            .filled_color(theme.status_color(metric.status))
            .empty_color(theme.muted),
        bar_area,
    );
}

/// Columns for the gauge's "NN%" share, plus the space separating it from the
/// bar.
const GAUGE_SHARE_WIDTH: u16 = 5;

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
