//! Persistent dashboard chrome: the error panel, the key-hint footer, and
//! the command-palette input bar.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::tui::{app::TuiApp, theme::Theme, ui::ADVANCED_LAYOUT_WIDTH};

/// Width thresholds at which the compact footer earns room for another hint.
/// Hints are added widest-terminal-last so a narrow terminal keeps the ones
/// a user cannot discover any other way (`q`/`Esc`).
const FOOTER_INTERVAL_HINT_WIDTH: u16 = 56;
const FOOTER_PALETTE_HINT_WIDTH: u16 = 72;
const FOOTER_VIEW_HINTS_WIDTH: u16 = 90;

pub(super) fn render_error(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let message = app.current_error.as_deref().unwrap_or("unknown error");
    let title = if app.is_fetching {
        "Retrying After Error"
    } else {
        "Current Error"
    };
    // While a retry is in flight the error is history, not current state, so
    // it is labeled as the previous failure instead of a live one.
    let paragraph = if app.is_fetching {
        Paragraph::new(vec![
            Line::from(Span::styled(
                "Retrying now; previous error:",
                theme.error_style(),
            )),
            Line::from(Span::styled(message, theme.error_style())),
        ])
    } else {
        Paragraph::new(message).style(theme.error_style())
    };

    frame.render_widget(
        paragraph
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(theme.panel_style())
                    .title(Span::styled(title, theme.error_style())),
            )
            .wrap(Wrap { trim: true }),
        area,
    );
}

pub(super) fn render_footer(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    if area.width >= ADVANCED_LAYOUT_WIDTH {
        render_showcase_footer(frame, area, theme);
        return;
    }

    let mut spans = vec![
        Span::styled("r", theme.value_style()),
        Span::raw(" refresh   "),
    ];

    if area.width >= FOOTER_INTERVAL_HINT_WIDTH {
        spans.extend([
            Span::styled("+/-", theme.value_style()),
            Span::raw(" interval   "),
        ]);
    }

    if area.width >= FOOTER_PALETTE_HINT_WIDTH {
        spans.extend([
            Span::styled(":", theme.value_style()),
            Span::raw(" palette   "),
        ]);
    }

    if area.width >= FOOTER_VIEW_HINTS_WIDTH {
        spans.extend([
            Span::styled("t", theme.value_style()),
            Span::raw(" theme   "),
            Span::styled("c", theme.value_style()),
            Span::raw(" config   "),
            Span::styled("space", theme.value_style()),
            Span::raw(" keys   "),
        ]);
    }

    spans.extend([
        Span::styled("q", theme.value_style()),
        Span::raw(" quit   "),
        Span::styled("Esc", theme.value_style()),
        Span::raw(" quit"),
    ]);

    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .style(theme.muted_style())
            .block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn render_showcase_footer(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let line = Line::from(Span::styled(
        "Latest measurements loaded.   space keys   r refresh   +/- interval   : palette   t theme   c config   q/Esc quit",
        theme.label_style(),
    ));

    frame.render_widget(
        Paragraph::new(line)
            .style(theme.footer_style())
            .alignment(Alignment::Left),
        area,
    );
}

/// Bottom overlay bar for the `:` command palette — drawn on top of the
/// live dashboard so values stay visible while typing a URL/interval.
pub(super) fn render_command_palette(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let line = match &app.palette_message {
        Some((message, is_error)) => Line::from(Span::styled(
            message.clone(),
            if *is_error {
                theme.error_style()
            } else {
                Style::default()
                    .fg(theme.success)
                    .add_modifier(Modifier::BOLD)
            },
        )),
        None => Line::from(vec![
            Span::styled(":", theme.command_style()),
            Span::raw(format!(" {}", app.palette_input)),
            Span::styled("█", theme.value_style()),
        ]),
    };

    frame.render_widget(Paragraph::new(line).style(theme.panel_style()), area);
}
