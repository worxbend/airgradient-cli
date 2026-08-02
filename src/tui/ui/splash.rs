//! The startup splash animation.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph},
};

use crate::tui::theme::{self, Theme};

const WORDMARK: &str = "AirGradient";
const TAGLINE: &str = "Air quality, at a glance";

/// How fast the shimmer travels across the wordmark, and how far the phase
/// shifts per letter. Tuned so one highlight sweep spans the whole word over
/// roughly the splash's frame budget.
const SHIMMER_SPEED_PER_FRAME: f64 = 0.35;
const SHIMMER_PHASE_PER_LETTER: f64 = 0.9;

/// Startup splash: a per-letter shimmer wordmark (blending `fg` toward
/// `accent` on a traveling phase), a tagline, and a fill gauge over the
/// frame budget. Skippable by any keypress (see `runtime::run_splash`).
pub(super) fn render_splash(frame: &mut Frame<'_>, area: Rect, theme: Theme, frame_no: u64) {
    let block = Block::default()
        .borders(Borders::ALL)
        .style(theme.panel_style())
        .border_style(theme.active_border_style());
    let box_area = centered_splash_box(area);
    let inner = block.inner(box_area);
    frame.render_widget(block, box_area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(shimmer_wordmark(theme, frame_no))).alignment(Alignment::Center),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(Span::styled(TAGLINE, theme.muted_style())).alignment(Alignment::Center),
        rows[2],
    );

    let ratio = (frame_no as f64 / theme::SPLASH_TOTAL_FRAMES as f64).clamp(0.0, 1.0);
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(theme.accent).bg(theme.border))
            .ratio(ratio)
            .label(""),
        center_horizontally(rows[3], 24),
    );
}

/// The splash panel, centered, shrinking to fit terminals smaller than its
/// preferred size but never below the minimum that still fits the wordmark —
/// and never larger than the area itself, which would draw outside the buffer.
fn centered_splash_box(area: Rect) -> Rect {
    let width = 46u16
        .min(area.width.saturating_sub(2))
        .max(10)
        .min(area.width);
    let height = 8u16
        .min(area.height.saturating_sub(2))
        .max(5)
        .min(area.height);

    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn shimmer_wordmark(theme: Theme, frame_no: u64) -> Vec<Span<'static>> {
    let phase = frame_no as f64 * SHIMMER_SPEED_PER_FRAME;

    WORDMARK
        .chars()
        .enumerate()
        .map(|(index, letter)| {
            let offset = phase + index as f64 * SHIMMER_PHASE_PER_LETTER;
            let blend = (0.5 + 0.5 * offset.sin()).clamp(0.0, 1.0) as f32;
            Span::styled(
                letter.to_string(),
                Style::default()
                    .fg(shimmer(theme.fg, theme.accent, blend))
                    .add_modifier(Modifier::BOLD),
            )
        })
        .collect()
}

fn center_horizontally(area: Rect, width: u16) -> Rect {
    let width = width.min(area.width);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y,
        width,
        height: area.height,
    }
}

/// Blends `base` toward `accent` by `t`. Only RGB themes can be interpolated;
/// named/indexed terminal colors have no channel values to mix, so those
/// themes keep a static wordmark rather than flickering between palette slots.
fn shimmer(base: Color, accent: Color, t: f32) -> Color {
    let (Color::Rgb(br, bg, bb), Color::Rgb(ar, ag, ab)) = (base, accent) else {
        return base;
    };
    let mix = |from: u8, to: u8| -> u8 {
        let from = f32::from(from);
        let to = f32::from(to);
        (from + (to - from) * t).round().clamp(0.0, 255.0) as u8
    };

    Color::Rgb(mix(br, ar), mix(bg, ag), mix(bb, ab))
}
