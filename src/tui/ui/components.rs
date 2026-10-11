//! Shared visual vocabulary. Components consume semantic intent, never palette literals.

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph},
};

use crate::tui::theme::{Theme, VisualState};

pub(super) fn panel(theme: Theme, title: &str, state: VisualState) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_set(symbols::border::ROUNDED)
        .border_style(if state == VisualState::Normal {
            theme.border_style()
        } else {
            theme.state_style(state)
        })
        .style(theme.surface_style())
        .title(Span::styled(format!(" {title} "), theme.title_style()))
}

pub(super) fn badge(theme: Theme, label: impl Into<String>, state: VisualState) -> Span<'static> {
    let label = label.into();
    let text = if theme.nerd_icons {
        format!(" {label} ")
    } else {
        format!("[{label}]")
    };
    Span::styled(text, theme.badge_style(theme.state_color(state)))
}

/// Keep key and description adjacent in text dumps as well as styled terminals.
pub(super) fn key_hint(theme: Theme, key: &str, label: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(key.to_owned(), theme.key_style()),
        Span::styled(format!(" {label}"), theme.muted_style()),
    ]
}

pub(super) fn section(theme: Theme, label: &str, detail: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(label.to_owned(), theme.title_style()),
        Span::styled(format!("  /  {detail}"), theme.muted_style()),
    ])
}

pub(super) fn tab(theme: Theme, label: &str, active: bool) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        if active {
            theme.selected_style()
        } else {
            theme.muted_style()
        },
    )
}

/// A bounded progress meter, with an eighth-cell tip only in the Unicode theme.
pub(super) fn meter(theme: Theme, ratio: f64, width: u16, state: VisualState) -> Line<'static> {
    let ratio = if ratio.is_finite() {
        ratio.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let filled = ratio * f64::from(width);
    let color = theme.state_color(state);
    let spans = (0..width)
        .map(|column| {
            let remaining = (filled - f64::from(column)).clamp(0.0, 1.0);
            let symbol = if !theme.nerd_icons {
                if remaining >= 0.5 { "#" } else { "-" }
            } else if remaining >= 1.0 {
                "━"
            } else if remaining > 0.0 {
                [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉"][(remaining * 8.0) as usize]
            } else {
                "─"
            };
            Span::styled(
                symbol,
                Style::default().fg(if remaining > 0.0 {
                    color
                } else {
                    theme.track()
                }),
            )
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

/// Clear and repaint overlays: Clear alone introduces terminal-default holes.
pub(super) fn dialog(frame: &mut Frame<'_>, area: Rect, theme: Theme, title: &str) -> Rect {
    frame.render_widget(Clear, area);
    let block = panel(theme, title, VisualState::Focused)
        .style(theme.raised_style())
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

/// Keep the cursor visible without starting a viewport inside a wide grapheme.
/// Cell-offset scrolling can round across that boundary and clip the final cell.
pub(super) fn render_input(frame: &mut Frame<'_>, area: Rect, line: Line<'_>, style: Style) {
    if line.width() <= usize::from(area.width) {
        frame.render_widget(Paragraph::new(line).style(style), area);
        return;
    }
    let graphemes = line.styled_graphemes(style).collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut width = 0;
    for grapheme in graphemes.into_iter().rev() {
        let span = Span::styled(grapheme.symbol, grapheme.style);
        width += span.width();
        if width > usize::from(area.width) {
            break;
        }
        spans.push(span);
    }
    spans.reverse();
    frame.render_widget(Paragraph::new(Line::from(spans)).style(style), area);
}

/// Log-style feedback shared by errors and command results; labels carry severity without color.
pub(super) fn log_line(
    theme: Theme,
    label: &str,
    message: &str,
    state: VisualState,
) -> Line<'static> {
    Line::from(vec![
        badge(theme, label, state),
        Span::styled(format!(" {message}"), theme.state_style(state)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_is_bounded_and_mono_has_no_block_elements() {
        for ratio in [f64::NAN, -1.0, 0.0, 0.17, 1.0, 2.0] {
            let line = meter(Theme::by_id("mono"), ratio, 12, VisualState::Active);
            assert_eq!(line.width(), 12);
            assert!(line.spans.iter().all(|span| span.content.is_ascii()));
        }
        assert_eq!(
            meter(Theme::default_theme(), 0.5, 0, VisualState::Active).width(),
            0
        );
    }
}
