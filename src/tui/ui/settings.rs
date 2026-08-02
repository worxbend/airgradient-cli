//! The two full-screen views: the theme picker and the config editor.
//!
//! Both take over the whole frame (no dashboard behind them) and both edit a
//! draft that is only written back to disk on confirm — see
//! [`crate::tui::app::TuiApp`] for the draft state and Esc/Enter semantics.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::{
    sensors::Status,
    tui::{
        app::{ConfigField, TuiApp},
        theme::{self, Theme},
        ui::hit::{HitMap, HitTarget},
    },
};

/// Column width reserved for config field labels, so values line up.
const CONFIG_LABEL_WIDTH: usize = 22;

/// Full-screen theme picker, styled after obsctl-rs/twi's settings view:
/// arrow keys live-preview a theme across the whole UI, Enter confirms and
/// persists it, Esc reverts to whatever was active before opening this view.
pub(super) fn render_theme_settings(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    hits: &mut HitMap,
) {
    let theme = app.theme;

    let outer = Block::default()
        .borders(Borders::ALL)
        .style(theme.panel_style())
        .border_style(theme.active_border_style())
        .title(
            " Themes — j/k preview \u{00b7} gg/G ends \u{00b7} Enter apply \u{00b7} Esc cancel ",
        );
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(inner);

    render_theme_list(frame, sections[0], app, hits);
    render_theme_preview(frame, sections[1], theme);
}

fn render_theme_list(frame: &mut Frame<'_>, area: Rect, app: &TuiApp, hits: &mut HitMap) {
    let theme = app.theme;
    let lines: Vec<Line> = theme::ALL
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let swatch = vec![
                Span::styled("██", Style::default().fg(candidate.accent)),
                Span::styled("██", Style::default().fg(candidate.success)),
                Span::styled("██", Style::default().fg(candidate.warning)),
                Span::styled("██", Style::default().fg(candidate.danger)),
                Span::raw("  "),
                Span::raw(candidate.label),
            ];
            let style = if index == app.settings_cursor {
                selected_row_style(theme)
            } else {
                Style::default()
            };
            Line::from(swatch).style(style)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .style(theme.panel_style())
        .border_style(theme.border_style())
        .title(" Themes ");
    let inner = block.inner(area);

    frame.render_widget(
        Paragraph::new(lines)
            .style(theme.panel_style())
            .block(block),
        area,
    );

    record_row_hits(hits, inner, theme::ALL.len(), HitTarget::ThemeRow);
}

/// Registers one click zone per visible list row.
///
/// Rows past the bottom of `inner` are skipped rather than recorded off-screen:
/// a click there lands on whatever is actually drawn, not on a row the user
/// cannot see.
fn record_row_hits(
    hits: &mut HitMap,
    inner: Rect,
    row_count: usize,
    target: impl Fn(usize) -> HitTarget,
) {
    let visible = usize::from(inner.height).min(row_count);

    for index in 0..visible {
        hits.push(
            Rect {
                x: inner.x,
                y: inner.y.saturating_add(index as u16),
                width: inner.width,
                height: 1,
            },
            target(index),
        );
    }
}

/// Sample of the currently highlighted theme, covering every role the
/// dashboard uses (statuses, muted/label/value text, selection) so a theme
/// can be judged without applying it.
fn render_theme_preview(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .style(theme.panel_style())
        .border_style(theme.active_border_style())
        .title(format!(" Preview: {} ", theme.label));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled("AirGradient", theme.title_style())),
        Line::raw(""),
        Line::from(Span::styled(
            "● AQI 42 Good",
            Style::default().fg(theme.status_color(Status::Good)),
        )),
        Line::from(Span::styled(
            "● AQI 120 Moderate",
            Style::default().fg(theme.status_color(Status::Moderate)),
        )),
        Line::from(Span::styled(
            "● AQI 250 Unhealthy",
            Style::default().fg(theme.status_color(Status::Unhealthy)),
        )),
        Line::raw(""),
        Line::from(Span::styled("muted text", theme.muted_style())),
        Line::from(Span::styled("label text", theme.label_style())),
        Line::from(Span::styled("value text", theme.value_style())),
        Line::raw(""),
        Line::from(Span::styled(
            " selected row ",
            Style::default()
                .bg(theme.highlight_bg)
                .fg(theme.highlight_fg),
        )),
    ];

    frame.render_widget(Paragraph::new(lines).style(theme.panel_style()), inner);
}

/// Full-screen config editor: navigate fields with arrows, Enter edits a
/// text field / toggles a boolean / opens the theme picker / saves, Esc
/// cancels the current field edit (or discards the whole draft and closes).
pub(super) fn render_config_editor(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    hits: &mut HitMap,
) {
    let theme = app.theme;
    let outer = Block::default()
        .borders(Borders::ALL)
        .style(theme.panel_style())
        .border_style(theme.active_border_style())
        .title(" Config — j/k navigate \u{00b7} Enter edit/toggle \u{00b7} Esc cancel ");
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let mut rows: Vec<Line> = ConfigField::ALL
        .iter()
        .enumerate()
        .map(|(index, field)| config_field_row(app, *field, index == app.config_editor_cursor))
        .collect();

    rows.push(Line::raw(""));
    if let Some(error) = &app.config_editor_error {
        rows.push(Line::from(Span::styled(
            format!("error: {error}"),
            theme.error_style(),
        )));
    }

    frame.render_widget(
        Paragraph::new(rows)
            .style(theme.panel_style())
            .wrap(Wrap { trim: true }),
        inner,
    );

    record_row_hits(hits, inner, ConfigField::ALL.len(), HitTarget::ConfigRow);
}

fn config_field_row(app: &TuiApp, field: ConfigField, selected: bool) -> Line<'static> {
    let theme = app.theme;
    let editing = selected && app.config_editor_editing.is_some();
    let style = if selected {
        selected_row_style(theme)
    } else {
        theme.value_style()
    };

    Line::from(vec![
        Span::styled(
            format!(
                "{:<width$}",
                config_field_label(field),
                width = CONFIG_LABEL_WIDTH
            ),
            style,
        ),
        Span::styled(
            config_field_display_value(app, field, editing),
            if selected { style } else { theme.muted_style() },
        ),
    ])
}

fn selected_row_style(theme: Theme) -> Style {
    Style::default()
        .bg(theme.highlight_bg)
        .fg(theme.highlight_fg)
        .add_modifier(Modifier::BOLD)
}

fn config_field_label(field: ConfigField) -> &'static str {
    match field {
        ConfigField::ServerUrl => "Server URL",
        ConfigField::RefreshInterval => "Refresh interval (s)",
        ConfigField::NotificationsEnabled => "Notifications enabled",
        ConfigField::StartMinimized => "Start minimized",
        ConfigField::Theme => "Theme",
        ConfigField::SaveAndClose => "[ Save & Close ]",
    }
}

/// The value column for one field. While a field is being edited the live
/// input buffer replaces the saved value, with a block cursor appended.
fn config_field_display_value(app: &TuiApp, field: ConfigField, editing: bool) -> String {
    if editing {
        let buffer = app.config_editor_editing.as_deref().unwrap_or("");
        return format!("{buffer}█");
    }

    match field {
        ConfigField::ServerUrl => app
            .config_draft
            .server_url
            .clone()
            .unwrap_or_else(|| "(none)".to_string()),
        ConfigField::RefreshInterval => format!("{}s", app.config_draft.refresh_interval_secs),
        ConfigField::NotificationsEnabled => {
            bool_label(app.config_draft.notifications_enabled).to_string()
        }
        ConfigField::StartMinimized => bool_label(app.config_draft.start_minimized).to_string(),
        ConfigField::Theme => app.theme.label.to_string(),
        ConfigField::SaveAndClose => String::new(),
    }
}

fn bool_label(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}
