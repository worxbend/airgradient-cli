//! Startup identity and progress, driven only by the existing splash frames.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::{
    theme::{self, Theme, VisualState},
    ui::{components, gradient},
};

const WORDMARK: &str = "AirGradient";
const TAGLINE: &str = "Air quality, at a glance";

/// The caller owns the finite animation budget. This view never introduces a
/// clock, timer, or background animation once the cockpit is idle.
pub(super) fn render_splash(frame: &mut Frame<'_>, area: Rect, theme: Theme, frame_no: u64) {
    if area.is_empty() {
        return;
    }
    frame.render_widget(Paragraph::new("").style(theme.page_style()), area);
    let panel = centered_splash_box(area);
    let inner = if panel.width >= 16 && panel.height >= 5 {
        components::dialog(frame, panel, theme, "AIR QUALITY / COCKPIT")
    } else {
        frame.render_widget(Paragraph::new("").style(theme.raised_style()), panel);
        panel
    };
    if inner.is_empty() {
        return;
    }

    let wordmark = Line::from(gradient::gradient_spans(
        WORDMARK,
        theme.accent_alt,
        theme.accent,
        frame_no,
        true,
    ));
    let progress = components::meter(
        theme,
        progress_ratio(frame_no),
        inner.width.min(32),
        VisualState::Active,
    );
    let skip = Line::from(components::key_hint(theme, "[Any key]", "skip"));
    let mut lines = match inner.height {
        0 => Vec::new(),
        1 => vec![wordmark],
        2 => vec![wordmark, progress],
        3 => vec![wordmark, progress, skip],
        4..=8 => vec![
            wordmark,
            Line::styled(TAGLINE, theme.muted_style()),
            progress,
            skip,
        ],
        _ => vec![
            Line::styled("PERSONAL AIR TELEMETRY", theme.muted_style()),
            Line::default(),
            wordmark,
            Line::styled(TAGLINE, theme.muted_style()),
            Line::default(),
            progress,
            Line::from(vec![
                Span::styled("STARTING COCKPIT  ", theme.muted_style()),
                Span::styled(
                    format!(
                        "{:02} / {:02}",
                        frame_no.saturating_add(1).min(theme::SPLASH_TOTAL_FRAMES),
                        theme::SPLASH_TOTAL_FRAMES
                    ),
                    theme.title_style(),
                ),
            ]),
            Line::default(),
            skip,
        ],
    };
    for line in &mut lines {
        line.alignment = Some(Alignment::Center);
    }
    let height = lines.len() as u16;
    let content = Rect::new(
        inner.x,
        inner.y + inner.height.saturating_sub(height) / 2,
        inner.width,
        height,
    );
    frame.render_widget(Paragraph::new(lines).style(theme.raised_style()), content);
}

fn progress_ratio(frame_no: u64) -> f64 {
    // The runtime emits 0..TOTAL_FRAMES, so its last frame must be complete.
    frame_no.saturating_add(1).min(theme::SPLASH_TOTAL_FRAMES) as f64
        / theme::SPLASH_TOTAL_FRAMES as f64
}

/// Preserve centering and containment even in a one-cell or offset viewport.
fn centered_splash_box(area: Rect) -> Rect {
    let width = 58.min(area.width.saturating_sub(u16::from(area.width > 18) * 2));
    let height = 13.min(area.height.saturating_sub(u16::from(area.height > 7) * 2));
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend, style::Color};

    #[test]
    fn progress_completes_on_the_last_scheduled_frame_and_saturates() {
        assert!(progress_ratio(0) > 0.0);
        assert_eq!(progress_ratio(theme::SPLASH_TOTAL_FRAMES - 1), 1.0);
        assert_eq!(progress_ratio(u64::MAX), 1.0);
        for frame_no in 1..theme::SPLASH_TOTAL_FRAMES {
            assert!(progress_ratio(frame_no) > progress_ratio(frame_no - 1));
        }
    }

    #[test]
    fn centered_panel_never_escapes_an_offset_viewport() {
        for width in 0..80 {
            for height in 0..30 {
                let area = Rect::new(3, 5, width, height);
                let panel = centered_splash_box(area);
                assert!(panel.x >= area.x && panel.y >= area.y);
                assert!(panel.right() <= area.right() && panel.bottom() <= area.bottom());
            }
        }
    }

    #[test]
    fn splash_has_identity_progress_and_skip_at_normal_sizes() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| render_splash(frame, frame.area(), Theme::default_theme(), 7))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let text = (0..24)
            .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        for label in [
            WORDMARK,
            TAGLINE,
            "PERSONAL AIR TELEMETRY",
            "STARTING COCKPIT",
            "08 / 24",
            "Any key",
            "skip",
        ] {
            assert!(text.contains(label), "missing splash label: {label}");
        }
    }

    #[test]
    fn every_theme_handles_tiny_sizes_and_mono_avoids_rgb_and_block_meters() {
        for theme in theme::ALL {
            for (width, height) in [(1, 1), (12, 3), (20, 5), (36, 20), (80, 24)] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|frame| render_splash(frame, frame.area(), *theme, 3))
                    .unwrap();
                for cell in &terminal.backend().buffer().content {
                    assert!(
                        theme.is_background(cell.bg),
                        "{} leaked background {:?}",
                        theme.id,
                        cell.bg
                    );
                    if theme.id == "mono" {
                        assert!(
                            cell.symbol()
                                .chars()
                                .all(|ch| !('\u{2580}'..='\u{259f}').contains(&ch)),
                            "mono block glyph: {}",
                            cell.symbol()
                        );
                        assert!(!matches!(cell.fg, Color::Rgb(..)));
                        assert!(!matches!(cell.bg, Color::Rgb(..)));
                    }
                }
            }
        }
    }
}
