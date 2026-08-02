//! The `<Space>` which-key popup.
//!
//! Anchored bottom-right and sized to its content, so it hints at what the
//! next key does without covering the readings the user is watching.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::tui::app::{LEADER_BINDINGS, TuiApp};

/// Cells of padding between the popup and the screen edge.
const MARGIN: u16 = 2;

pub(super) fn render_leader_popup(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;

    let widest_label = LEADER_BINDINGS
        .iter()
        .map(|binding| binding.label.len())
        .max()
        .unwrap_or(0);
    // "<space> " prefix + key + separator + label, plus the border.
    let width = (widest_label as u16 + 14).min(area.width.saturating_sub(MARGIN * 2));
    let height = (LEADER_BINDINGS.len() as u16 + 2).min(area.height.saturating_sub(MARGIN));

    let popup = Rect {
        x: area
            .x
            .saturating_add(area.width.saturating_sub(width + MARGIN)),
        y: area
            .y
            .saturating_add(area.height.saturating_sub(height + MARGIN)),
        width,
        height,
    };

    let lines: Vec<Line> = LEADER_BINDINGS
        .iter()
        .map(|binding| {
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    binding.key.to_string(),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  →  ", theme.muted_style()),
                Span::styled(binding.label, theme.value_style()),
            ])
        })
        .collect();

    // `Clear` first: the dashboard underneath would otherwise show through.
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).style(theme.panel_style()).block(
            Block::default()
                .borders(Borders::ALL)
                .style(theme.panel_style())
                .border_style(theme.active_border_style())
                .title(Span::styled(" <space> ", theme.title_style())),
        ),
        popup,
    );
}
