//! Theme-driven air-quality notifications.
//!
//! The third-party toaster fixes its border colors and leaves the underlying
//! dashboard visible. This renderer uses the same raised panel and severity
//! tokens as the rest of the cockpit. The app's alert queue and expiry are
//! deliberately untouched: drawing is a pure projection of the current alert.

use ratatui::{
    Frame,
    layout::Rect,
    text::Line,
    widgets::{Clear, Padding, Paragraph, Wrap},
};

use crate::tui::{
    app::{AirAlert, AlertKind, TuiApp},
    theme::{Theme, VisualState},
    ui::components,
};

pub(super) fn render_alert(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let Some(alert) = app.current_alert() else {
        return;
    };
    let (state, label) = alert_presentation(alert);
    render_notification(frame, area, app.theme, label, &alert.message, state);
}

/// A reusable, stateless notification surface; severity is textual as well as
/// colored, so a monochrome terminal retains the meaning of an alert.
fn render_notification(
    frame: &mut Frame<'_>,
    area: Rect,
    theme: Theme,
    label: &str,
    message: &str,
    state: VisualState,
) {
    if area.is_empty() {
        return;
    }
    let margin = u16::from(area.width >= 36 && area.height >= 10);
    let width = (Line::raw(message).width().saturating_add(4).clamp(30, 58) as u16)
        .min(area.width.saturating_sub(margin * 2));
    let height = 7.min(area.height.saturating_sub(margin * 2));
    let popup = Rect::new(
        area.right().saturating_sub(width + margin),
        area.bottom().saturating_sub(height + margin),
        width,
        height,
    );

    frame.render_widget(Clear, popup);
    if width < 8 || height < 4 {
        frame.render_widget(
            Paragraph::new(message)
                .style(theme.raised_style().patch(theme.state_style(state)))
                .wrap(Wrap { trim: true }),
            popup,
        );
        return;
    }

    let block = components::panel(theme, "AIR QUALITY", state)
        .style(theme.raised_style())
        .padding(Padding::horizontal(1));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    frame.render_widget(
        Paragraph::new(Line::from(components::badge(
            theme,
            label.to_owned(),
            state,
        )))
        .style(theme.raised_style()),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    // Give long messages every available row on a short terminal. Normal
    // sizes leave a quiet line between the severity badge and the reading.
    let gap = u16::from(inner.height >= 5);
    let message_area = Rect::new(
        inner.x,
        inner.y + 1 + gap,
        inner.width,
        inner.height.saturating_sub(1 + gap),
    );
    frame.render_widget(
        Paragraph::new(message)
            .style(theme.raised_style())
            .wrap(Wrap { trim: true }),
        message_area,
    );
}

fn alert_presentation(alert: &AirAlert) -> (VisualState, &'static str) {
    use crate::sensors::Status;

    match alert.kind {
        AlertKind::Recovered => (VisualState::Success, "RECOVERED"),
        AlertKind::Worsened => match alert.status {
            Status::Unhealthy | Status::VeryUnhealthy => (VisualState::Error, "UNHEALTHY"),
            _ => (VisualState::Warning, "ATTENTION"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sensors::Status, tui::theme};
    use ratatui::{Terminal, backend::TestBackend, style::Color};
    use std::time::UNIX_EPOCH;

    fn alert(kind: AlertKind, status: Status) -> AirAlert {
        AirAlert {
            metric_key: "pm25",
            kind,
            status,
            message: "PM2.5 is Unhealthy — 58.2 ug/m3".to_string(),
            raised_at: UNIX_EPOCH,
        }
    }

    #[test]
    fn severity_matches_the_existing_alert_contract() {
        for status in [Status::Unhealthy, Status::VeryUnhealthy] {
            assert_eq!(
                alert_presentation(&alert(AlertKind::Worsened, status)).0,
                VisualState::Error
            );
        }
        assert_eq!(
            alert_presentation(&alert(AlertKind::Worsened, Status::Elevated)).0,
            VisualState::Warning
        );
        assert_eq!(
            alert_presentation(&alert(AlertKind::Recovered, Status::Good)).0,
            VisualState::Success
        );
    }

    #[test]
    fn notifications_clear_underlying_text_and_use_theme_surfaces() {
        for theme in theme::ALL {
            for state in [
                VisualState::Warning,
                VisualState::Error,
                VisualState::Success,
            ] {
                let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
                terminal
                    .draw(|frame| {
                        frame.render_widget(
                            Paragraph::new(vec![Line::raw("X".repeat(80)); 24])
                                .style(theme.page_style()),
                            frame.area(),
                        );
                        render_notification(
                            frame,
                            frame.area(),
                            *theme,
                            "NOTICE",
                            "PM2.5 is Unhealthy - 58.2 ug/m3",
                            state,
                        );
                    })
                    .unwrap();
                let buffer = terminal.backend().buffer();
                // The right-hand padding of the popup is cleared and repainted.
                assert_eq!(buffer[(77, 19)].symbol(), " ");
                assert_eq!(buffer[(77, 19)].bg, theme.surface_hi());
                for cell in &buffer.content {
                    assert!(
                        theme.is_background(cell.bg),
                        "{} leaked background {:?}",
                        theme.id,
                        cell.bg
                    );
                    if theme.id == "mono" {
                        assert!(!matches!(cell.fg, Color::Rgb(..)));
                        assert!(!matches!(cell.bg, Color::Rgb(..)));
                    }
                }
            }
        }
    }

    #[test]
    fn messages_wrap_and_tiny_notifications_do_not_panic() {
        for (width, height) in [(1, 1), (7, 3), (18, 5), (36, 20)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| {
                    render_notification(
                        frame,
                        frame.area(),
                        Theme::default_theme(),
                        "RECOVERED",
                        "CO2 back to Good - 420 ppm",
                        VisualState::Success,
                    )
                })
                .unwrap();
            if width >= 36 {
                let buffer = terminal.backend().buffer();
                let text = (0..height)
                    .map(|y| {
                        (0..width)
                            .map(|x| buffer[(x, y)].symbol())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(text.contains("RECOVERED"));
                assert!(text.contains("420 ppm"));
            }
        }
    }
}
