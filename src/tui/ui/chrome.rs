//! Dashboard controls and feedback. Input routing stays in the runtime.

use super::{components, widget_block};
use crate::tui::{
    app::{PaletteMessageKind, TuiApp},
    theme::{Theme, VisualState},
};
use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Padding, Paragraph, Wrap},
};

pub(super) fn render_error(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let message = app.current_error.as_deref().unwrap_or("unknown error");
    let (title, state) = if app.is_fetching {
        ("Retrying After Error", VisualState::Warning)
    } else {
        ("Current Error", VisualState::Error)
    };
    let mut lines = Vec::new();
    if app.is_fetching {
        lines.push(Line::from(Span::styled(
            "Retrying now; previous error:",
            theme.state_style(state),
        )));
    }
    lines.push(components::log_line(
        theme,
        if app.is_fetching { "RETRY" } else { "ERROR" },
        message,
        state,
    ));
    if !app.is_fetching {
        lines.push(Line::from(Span::styled(
            "r retry  ·  c configure",
            theme.muted_style(),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(components::panel(theme, title, state).padding(Padding::horizontal(1)))
            .wrap(Wrap { trim: true }),
        area,
    );
}

pub(super) fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let inner_width = area.width.saturating_sub(4);
    let mut spans = Vec::new();
    // Always reserve the exit controls before fitting optional shortcuts.
    let quit = "q quit   Esc quit";
    let mut used = 0;
    append_hint(&mut spans, theme, "r", "refresh", &mut used);
    for (key, label) in [
        ("+/-", "interval"),
        (":", "palette"),
        ("space", "keys"),
        ("h", "history"),
        ("t", "theme"),
        ("c", "config"),
    ] {
        let next = key.len() + label.len() + 4;
        if used + next + quit.len() <= usize::from(inner_width) {
            append_hint(&mut spans, theme, key, label, &mut used);
        }
    }
    spans.extend(components::key_hint(theme, "q", "quit"));
    spans.push(Span::raw("   "));
    spans.extend(components::key_hint(theme, "Esc", "quit"));
    let mut block = widget_block(theme).padding(Padding::horizontal(1));
    if area.width >= 112 {
        let status = if app.current_error.is_some() && app.current_snapshot.is_none() {
            "No successful measurements yet."
        } else if app.current_error.is_some() {
            "Last successful measurements retained."
        } else if app.is_fetching {
            "Fetching measurements…"
        } else if app.current_snapshot.is_some() {
            "Latest measurements loaded."
        } else {
            "Waiting for measurements."
        };
        block = block.title(Span::styled(format!(" {status} "), theme.muted_style()));
    }
    if let Some(message) = &app.palette_message {
        let state = match message.kind {
            PaletteMessageKind::Error => VisualState::Error,
            PaletteMessageKind::Success => VisualState::Success,
        };
        block = block.title_bottom(components::log_line(theme, "COMMAND", &message.text, state));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(theme.footer_style())
            .block(block),
        area,
    );
}

fn append_hint(
    spans: &mut Vec<Span<'static>>,
    theme: Theme,
    key: &str,
    label: &str,
    used: &mut usize,
) {
    spans.extend(components::key_hint(theme, key, label));
    spans.push(Span::raw("   "));
    *used += key.len() + label.len() + 4;
}
