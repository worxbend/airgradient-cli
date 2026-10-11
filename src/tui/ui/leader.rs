//! The `<Space>` command deck: discoverable keys on a raised surface.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use crate::tui::{
    app::{LEADER_BINDINGS, TuiApp},
    theme::Theme,
    ui::components,
};

/// Draw only; the app still owns leader arming, cancellation, and dispatch.
pub(super) fn render_leader_popup(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    render_commands(frame, area, app.theme);
}

fn render_commands(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    if area.is_empty() {
        return;
    }

    // Spend space on section labels only after every binding fits. A short,
    // wide terminal gets two columns rather than silently losing the last keys.
    let margin = u16::from(area.width >= 40 && area.height >= 18);
    let available_width = area.width.saturating_sub(margin * 2);
    let available_height = area.height.saturating_sub(margin * 2);
    let sections = available_height >= 17;
    let columns = if available_height < 12 && available_width >= 52 {
        2
    } else {
        1
    };
    let width = (if columns == 2 { 54 } else { 36 }).min(available_width);
    let mut lines = command_lines(theme, sections, columns);
    let height = (lines.len() as u16 + 2).min(available_height);
    let popup = Rect::new(
        area.right().saturating_sub(width + margin),
        area.bottom().saturating_sub(height + margin),
        width,
        height,
    );

    // Below the normal layout contract, give all keys a chance to remain
    // discoverable rather than showing only the first few complete labels.
    if height < lines.len() as u16 + 2 {
        let keys = LEADER_BINDINGS
            .iter()
            .map(|binding| binding.key.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        lines = vec![Line::styled(keys, theme.key_style())];
    }
    if popup.width < 6 || popup.height < 3 {
        frame.render_widget(Clear, popup);
        frame.render_widget(Paragraph::new(lines).style(theme.raised_style()), popup);
        return;
    }
    let inner = components::dialog(frame, popup, theme, "<space> / COMMANDS");
    frame.render_widget(Paragraph::new(lines).style(theme.raised_style()), inner);
}

fn command_lines(theme: Theme, sections: bool, columns: usize) -> Vec<Line<'static>> {
    if sections {
        let mut lines = vec![components::section(theme, "MONITOR", "live readings")];
        for (heading, detail, keys) in [
            ("", "", "rh"),
            ("WORKSPACE", "views & settings", "tc:"),
            ("SESSION", "refresh cadence", "+-q"),
        ] {
            if !heading.is_empty() {
                lines.push(Line::default());
                lines.push(components::section(theme, heading, detail));
            }
            for key in keys.chars() {
                if let Some(binding) = LEADER_BINDINGS.iter().find(|binding| binding.key == key) {
                    lines.push(Line::from(components::key_hint(
                        theme,
                        &format!("[{key}]"),
                        binding.label,
                    )));
                }
            }
        }
        lines.push(Line::default());
        lines.push(Line::from(components::key_hint(theme, "[Space]", "cancel")));
        return lines;
    }

    LEADER_BINDINGS
        .chunks(columns)
        .map(|bindings| {
            let mut spans = Vec::new();
            for (index, binding) in bindings.iter().enumerate() {
                let cell = Line::from(components::key_hint(
                    theme,
                    &format!("[{}]", binding.key),
                    binding.label,
                ));
                let padding = 25usize.saturating_sub(cell.width());
                spans.extend(cell.spans);
                if index + 1 < bindings.len() {
                    spans.push(Span::raw(" ".repeat(padding)));
                }
            }
            Line::from(spans)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn all_commands_fit_minimum_normal_and_short_wide_terminals() {
        for (width, height) in [(36, 20), (80, 18), (80, 19), (80, 24), (100, 40), (80, 8)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| render_commands(frame, frame.area(), Theme::default_theme()))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let text = (0..height)
                .map(|y| {
                    (0..width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            for binding in LEADER_BINDINGS {
                assert!(
                    text.contains(binding.label),
                    "{width}x{height} lost {}",
                    binding.label
                );
            }
        }
    }

    #[test]
    fn leader_surfaces_clear_underlying_content_in_every_theme() {
        for theme in crate::tui::theme::ALL {
            let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
            terminal
                .draw(|frame| {
                    frame.render_widget(
                        Paragraph::new(vec![Line::raw("X".repeat(80)); 24])
                            .style(theme.page_style()),
                        frame.area(),
                    );
                    render_commands(frame, frame.area(), *theme);
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            assert_eq!(buffer[(77, 10)].symbol(), " ");
            assert_eq!(buffer[(77, 10)].bg, theme.surface_hi());
            assert!(
                buffer
                    .content
                    .iter()
                    .all(|cell| theme.is_background(cell.bg))
            );
        }
    }

    #[test]
    fn mono_help_avoids_nerd_font_glyphs_and_tiny_sizes_are_safe() {
        for (width, height) in [(1, 1), (4, 2), (12, 4), (36, 20)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| render_commands(frame, frame.area(), Theme::by_id("mono")))
                .unwrap();
            for cell in &terminal.backend().buffer().content {
                assert!(
                    cell.symbol()
                        .chars()
                        .all(|ch| !('\u{e000}'..='\u{f8ff}').contains(&ch)),
                    "unexpected mono glyph: {}",
                    cell.symbol()
                );
            }
        }
    }
}
