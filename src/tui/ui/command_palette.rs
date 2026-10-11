//! Command dialog: the existing verb grammar presented as a readable command reference.
//! This is not a second command router; submission still belongs to app::palette.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::{Line, Span},
    widgets::{Cell, Paragraph, Row, Table},
};

use super::components;
use crate::tui::{app::TuiApp, theme::VisualState};

const COMMANDS: &[(&str, &str)] = &[
    ("url <URL>", "Connect to your local monitor"),
    ("refresh <SECONDS>", "Set the refresh interval (5–3600)"),
    ("theme <ID>", "Apply a palette by its id"),
    ("themes", "Browse and preview all themes"),
    ("config", "Edit device and preferences"),
    ("save", "Save current session settings"),
    ("quit", "Return to your terminal"),
];

pub(super) fn render(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let width = area.width.saturating_sub(4).min(78);
    let height = area.height.saturating_sub(4).min(15);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    );
    let inner = components::dialog(frame, popup, theme, "COMMAND / Type a command");
    let [input, _, reference, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);

    let line = Line::from(vec![
        Span::styled(": ", theme.key_style()),
        Span::styled(app.palette_input.clone(), theme.value_style()),
        Span::styled(
            if theme.nerd_icons { "▎" } else { "_" },
            theme.title_style(),
        ),
    ]);
    components::render_input(frame, input, line, theme.selected_style());

    let query = app
        .palette_input
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    let narrow = reference.width < 48;
    let rows = COMMANDS
        .iter()
        .filter(|(verb, _)| verb.starts_with(&query))
        .map(|(verb, description)| {
            Row::new(vec![
                Cell::from(*verb).style(theme.state_style(VisualState::Info)),
                Cell::from(if narrow { "" } else { *description }).style(theme.muted_style()),
            ])
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("Enter runs the typed command; Esc cancels.").style(theme.muted_style()),
            reference,
        );
    } else {
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(if narrow { reference.width } else { 22 }),
                    Constraint::Min(0),
                ],
            )
            .column_spacing(if narrow { 0 } else { 2 })
            .style(theme.raised_style()),
            reference,
        );
    }
    let mut hints = components::key_hint(theme, "Enter", "run");
    hints.push(Span::raw("   "));
    hints.extend(components::key_hint(theme, "Esc", "cancel"));
    if footer.width >= 47 {
        hints.push(Span::raw("   "));
        hints.extend(components::key_hint(theme, "Ctrl-u", "clear"));
    }
    frame.render_widget(Paragraph::new(Line::from(hints)), footer);
}
