//! Surface-based telemetry cards and a typographic AQI hero. Icons are small
//! title accents, not empty columns competing with the actual measurements.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, Wrap},
};

use crate::{
    sensors::{MISSING_VALUE, Metric, Status},
    tui::{
        app::TuiApp,
        theme::{Theme, VisualState},
        ui::{components, format::aqi_message, gauge, history},
    },
};

#[derive(Clone, Copy)]
pub(super) struct CardSpec {
    pub(super) title: &'static str,
}

const HERO_TEXT_ROWS: u16 = 5;

pub(super) fn render_air_monitor_aqi_card(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    metric: &Metric,
    readings: &[Option<f64>],
) {
    let theme = app.theme;
    let block = card_block(theme, "Air Quality Index", metric);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Text, history, and the full scale each own their rows. Large typography
    // uses horizontal space, never rows borrowed from the gauges or PM cards.
    let gauge_rows = gauge::FULL_HEIGHT.min(inner.height.saturating_sub(HERO_TEXT_ROWS));
    let history_rows =
        history::history_rows_beside_text(inner.height.saturating_sub(gauge_rows), HERO_TEXT_ROWS);
    let [headline, history_area, gauge_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(HERO_TEXT_ROWS),
            Constraint::Length(history_rows),
            Constraint::Length(gauge_rows),
        ])
        .areas(inner);

    let digits = large_digits(metric.display_value());
    let large = theme.nerd_icons
        && headline.width >= 48
        && headline.height >= HERO_TEXT_ROWS
        && digits.is_some();
    let text_area = if large {
        let [number_area, text_area] = Layout::default()
            .direction(Direction::Horizontal)
            .spacing(2)
            .constraints([Constraint::Length(15), Constraint::Min(28)])
            .areas(headline);
        let mut number = vec![Line::from(Span::styled("CURRENT AQI", theme.muted_style()))];
        if let Some(digits) = digits {
            number.extend(digits.into_iter().map(|line| {
                Line::from(Span::styled(
                    line,
                    theme
                        .status_style(metric.status)
                        .add_modifier(Modifier::BOLD),
                ))
            }));
        }
        frame.render_widget(Paragraph::new(number), number_area);
        text_area
    } else {
        headline
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(status_badge(theme, metric.status)),
            Line::from(vec![
                Span::styled("AQI ", theme.label_style()),
                Span::styled(metric.display_value(), theme.value_style()),
            ]),
            Line::from(Span::styled(aqi_message(app), theme.label_style())),
            Line::from(trend_spans(theme, metric)),
        ])
        .wrap(Wrap { trim: true }),
        text_area,
    );
    history::render_metric_history(frame, history_area, theme, metric, readings);
    gauge::render_band_gauge(frame, gauge_area, theme, metric);
}

pub(super) fn render_air_monitor_metric_card(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: Theme,
    metric: &Metric,
    spec: CardSpec,
) {
    let block = card_block(theme, spec.title, metric);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    let gauge_rows = u16::from(inner.height >= 3);
    let [text, meter] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(2), Constraint::Length(gauge_rows)])
        .areas(inner);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(metric.display_value(), theme.value_style()),
            Span::styled(
                if metric.unit.is_empty() {
                    String::new()
                } else {
                    format!(" {}", metric.unit)
                },
                theme.label_style(),
            ),
        ]),
        Line::from(status_badge(theme, metric.status)),
    ];
    if text.height >= 3 {
        lines.push(Line::from(trend_spans(theme, metric)));
    }
    // Values deliberately never wrap; the full width is available now that
    // icons live in the title. The largest count still fits the narrow PM card.
    frame.render_widget(Paragraph::new(lines), text);
    render_metric_gauge(frame, meter, theme, metric);
}

fn status_badge(theme: Theme, status: Status) -> Span<'static> {
    let label = if theme.nerd_icons {
        format!(" {} ", status.label())
    } else {
        format!("[{}]", status.label())
    };
    Span::styled(label, theme.badge_style(theme.status_color(status)))
}

fn trend_spans(theme: Theme, metric: &Metric) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            metric.trend.display_symbol(),
            theme.trend_style(metric.trend),
        ),
        Span::styled(" from last reading", theme.muted_style()),
    ]
}

/// Keep the percentage and empty track for missing readings, but never paint
/// a missing sensor as a filled zero measurement.
fn render_metric_gauge(frame: &mut Frame<'_>, area: Rect, theme: Theme, metric: &Metric) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let value = metric.value.unwrap_or(0.0).clamp(0.0, metric.scale.max());
    let share = match metric.value {
        Some(_) => format!("{:>3.0}% ", (value / metric.scale.max()) * 100.0),
        None => format!("{MISSING_VALUE:>4} "),
    };
    let [label, bar] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(5), Constraint::Min(0)])
        .areas(area);
    frame.render_widget(
        Paragraph::new(Span::styled(share, theme.muted_style())),
        label,
    );
    let width = usize::from(bar.width);
    let filled = ((value / metric.scale.max()) * f64::from(bar.width)).round() as usize;
    let (filled_symbol, empty_symbol) = if theme.nerd_icons {
        ("━", "─")
    } else {
        ("#", "-")
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                filled_symbol.repeat(filled),
                theme.status_style(metric.status),
            ),
            Span::styled(
                empty_symbol.repeat(width.saturating_sub(filled)),
                theme.border_style().fg(theme.track()),
            ),
        ])),
        bar,
    );
}

fn card_block(theme: Theme, title: &str, metric: &Metric) -> Block<'static> {
    components::panel(
        theme,
        &format!("{} {title}", theme.icon(metric.icon)),
        VisualState::Normal,
    )
    .border_style(theme.border_style())
    .style(theme.card_surface(metric.status))
    .padding(Padding::horizontal(1))
}

/// Three-row display digits. The plain numeric value is always drawn beside
/// them, and missing or non-integer values stay in the normal text treatment.
fn large_digits(value: &str) -> Option<[String; 3]> {
    const DIGITS: [[&str; 3]; 10] = [
        ["█▀█", "█ █", "█▄█"],
        [" ▀█", "  █", "  █"],
        ["▀▀█", "█▀▀", "█▄▄"],
        ["▀▀█", " ▀█", "▄▄█"],
        ["█ █", "█▄█", "  █"],
        ["█▀▀", "▀▀█", "▄▄█"],
        ["█▀▀", "█▀█", "█▄█"],
        ["▀▀█", "  █", "  █"],
        ["█▀█", "█▀█", "█▄█"],
        ["█▀█", "▀▀█", "▄▄█"],
    ];
    if value.is_empty() || value.len() > 3 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(std::array::from_fn(|row| {
        value
            .bytes()
            .map(|byte| DIGITS[usize::from(byte - b'0')][row])
            .collect::<Vec<_>>()
            .join(" ")
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::{SensorSnapshot, metrics};
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn display_digits_only_accept_short_integer_readings() {
        for value in ["0", "42", "500"] {
            let rows = large_digits(value).expect("integer display");
            assert!(
                rows.iter()
                    .all(|row| row.chars().count() == value.len() * 4 - 1)
            );
        }
        for value in ["", "--", "-1", "1.5", "1000"] {
            assert!(large_digits(value).is_none());
        }
    }

    #[test]
    fn missing_metric_has_no_filled_meter_and_mono_uses_ascii() {
        let theme = Theme::by_id("mono");
        for value in [None, Some(0.0), Some(800.0)] {
            let built = metrics(
                &SensorSnapshot {
                    co2: value,
                    ..SensorSnapshot::default()
                },
                None,
            );
            let metric = built.by_key("co2").expect("metric");
            let mut terminal = Terminal::new(TestBackend::new(20, 1)).expect("terminal");
            terminal
                .draw(|frame| render_metric_gauge(frame, frame.area(), theme, metric))
                .expect("render");
            let buffer = terminal.backend().buffer();
            let output = (0..20).map(|x| buffer[(x, 0)].symbol()).collect::<String>();
            assert!(output.is_ascii());
            if value == Some(800.0) {
                assert!(output.contains('#'));
            } else {
                assert!(!output.contains('#'));
            }
            assert_eq!(output.contains('%'), value.is_some());
        }
    }

    #[test]
    fn card_surface_follows_the_reading_not_the_metric_category() {
        let theme = Theme::default_theme();
        for value in [None, Some(400.0), Some(1900.0)] {
            let built = metrics(
                &SensorSnapshot {
                    co2: value,
                    ..SensorSnapshot::default()
                },
                None,
            );
            let metric = built.by_key("co2").expect("metric");
            let mut terminal = Terminal::new(TestBackend::new(28, 6)).expect("terminal");
            terminal
                .draw(|frame| {
                    render_air_monitor_metric_card(
                        frame,
                        frame.area(),
                        theme,
                        metric,
                        CardSpec { title: "CO₂" },
                    )
                })
                .expect("render");
            assert_eq!(
                terminal.backend().buffer()[(26, 4)].bg,
                theme
                    .card_surface(metric.status)
                    .bg
                    .expect("surface background")
            );
        }
    }
}
