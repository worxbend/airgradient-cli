//! Gradient text: a run of characters coloured along a ramp, optionally with
//! the ramp travelling through them.
//!
//! Shared by the splash wordmark and the dashboard's, so the two shimmer the
//! same way rather than each carrying their own copy of the maths.
//!
//! The animation is driven by a caller-supplied phase rather than by a clock,
//! because the runtime only advances that phase while something is actually
//! happening (see `TuiApp::is_animating`). A still dashboard therefore shows a
//! still gradient — which is the point: this is decoration, and decoration that
//! repaints ten times a second all day is a cost with no benefit.

use ratatui::{
    style::{Color, Modifier, Style},
    text::Span,
};

/// How far the ramp shifts between neighbouring characters. Tuned so a short
/// word shows most of the ramp at once rather than one flat colour.
const PHASE_PER_CHARACTER: f64 = 0.55;

/// How far the ramp travels per animation frame.
const PHASE_PER_FRAME: f64 = 0.30;

/// Colours `text` along a ramp from `from` to `to`, travelling with `frame`.
///
/// Pass a constant `frame` for a still gradient; the ramp still varies across
/// the characters, so the word is coloured either way.
pub(super) fn gradient_spans(
    text: &str,
    from: Color,
    to: Color,
    frame: u64,
    bold: bool,
) -> Vec<Span<'static>> {
    let phase = frame as f64 * PHASE_PER_FRAME;

    text.chars()
        .enumerate()
        .map(|(index, character)| {
            let offset = phase + index as f64 * PHASE_PER_CHARACTER;
            // `sin` maps to -1..1; this puts it in 0..1 so it can be a blend.
            let mix = (0.5 + 0.5 * offset.sin()).clamp(0.0, 1.0) as f32;
            let mut style = Style::default().fg(blend(from, to, mix));
            if bold {
                style = style.add_modifier(Modifier::BOLD);
            }

            Span::styled(character.to_string(), style)
        })
        .collect()
}

pub(super) use crate::tui::theme::blend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_span_per_character_carrying_the_character() {
        let spans = gradient_spans(
            "AQI",
            Color::Rgb(0, 0, 0),
            Color::Rgb(255, 255, 255),
            0,
            false,
        );

        assert_eq!(spans.len(), 3);
        assert_eq!(
            spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>(),
            "AQI"
        );
    }

    #[test]
    fn neighbouring_characters_differ_so_the_ramp_is_visible() {
        let spans = gradient_spans(
            "AirGradient",
            Color::Rgb(0, 0, 0),
            Color::Rgb(255, 255, 255),
            0,
            false,
        );
        let colours: Vec<_> = spans.iter().map(|span| span.style.fg).collect();

        assert!(
            colours.windows(2).any(|pair| pair[0] != pair[1]),
            "a gradient that is one flat colour is not a gradient"
        );
    }

    #[test]
    fn advancing_the_frame_moves_the_ramp() {
        let still = gradient_spans(
            "AirGradient",
            Color::Rgb(0, 0, 0),
            Color::Rgb(255, 255, 255),
            0,
            false,
        );
        let later = gradient_spans(
            "AirGradient",
            Color::Rgb(0, 0, 0),
            Color::Rgb(255, 255, 255),
            4,
            false,
        );

        assert_ne!(
            still.iter().map(|s| s.style.fg).collect::<Vec<_>>(),
            later.iter().map(|s| s.style.fg).collect::<Vec<_>>()
        );
    }

    #[test]
    fn named_terminal_colours_are_left_alone() {
        // The TTY-safe theme must not flicker between palette slots.
        let spans = gradient_spans("AQI", Color::White, Color::Cyan, 3, false);

        for span in spans {
            assert_eq!(span.style.fg, Some(Color::White));
        }
    }

    #[test]
    fn blending_hits_both_ends_exactly() {
        let black = Color::Rgb(0, 0, 0);
        let white = Color::Rgb(255, 255, 255);

        assert_eq!(blend(black, white, 0.0), black);
        assert_eq!(blend(black, white, 1.0), white);
        assert_eq!(blend(black, white, 0.5), Color::Rgb(128, 128, 128));
    }
}
