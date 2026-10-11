//! Full-screen draft settings. Rendering never commits a theme or config.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Padding, Paragraph, Wrap},
};

use crate::tui::{
    app::{ConfigField, TuiApp},
    theme::{self, Theme, VisualState},
    ui::{
        components::{badge, key_hint, meter, panel, render_input, section},
        hit::{HitMap, HitTarget},
        screen_inset, widget_gap,
    },
};

/// Column width reserved for config field labels, so values line up.
const CONFIG_LABEL_WIDTH: usize = 22;

pub(super) fn render_theme_settings(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    hits: &mut HitMap,
) {
    let theme = app.theme;
    let area = screen_inset(area);
    let outer = panel(theme, "Themes", VisualState::Focused).padding(Padding::horizontal(1));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    let [heading, content, controls] = settings_regions(inner);
    frame.render_widget(
        Paragraph::new(section(theme, "APPEARANCE", "Live preview")),
        heading,
    );
    render_controls(frame, controls, theme, "preview", "apply");

    // Keep the names readable before spending columns on a second panel.
    if content.width < 72 {
        render_theme_list(frame, content, app, hits);
    } else {
        let [list, preview] = Layout::default()
            .direction(Direction::Horizontal)
            .spacing(widget_gap(content))
            .constraints([Constraint::Length(38), Constraint::Min(30)])
            .areas(content);
        render_theme_list(frame, list, app, hits);
        render_theme_preview(frame, preview, theme);
    }
}

fn settings_regions(inner: Rect) -> [Rect; 3] {
    // At very short sizes the list wins over explanatory copy.
    let heading = if inner.height >= 8 { 2 } else { 0 };
    let controls = if inner.height >= 5 { 2 } else { 0 };
    Layout::vertical([
        Constraint::Length(heading),
        Constraint::Min(1),
        Constraint::Length(controls),
    ])
    .areas(inner)
}

fn render_controls(frame: &mut Frame<'_>, area: Rect, theme: Theme, nav: &str, enter: &str) {
    let mut first = key_hint(theme, "j/k", nav);
    first.push(Span::raw("   "));
    first.extend(key_hint(theme, "Enter", enter));
    let mut second = key_hint(theme, "gg/G", "ends");
    second.push(Span::raw("   "));
    second.extend(key_hint(theme, "Esc", "cancel"));
    if area.width >= 64 {
        first.push(Span::raw("   "));
        first.extend(second);
        second = Vec::new();
    }
    frame.render_widget(
        Paragraph::new(vec![Line::from(first), Line::from(second)]).style(theme.surface_style()),
        area,
    );
}

/// A deterministic scroll window, including when a resize hides the selection.
/// The model's cursor remains absolute; only the visible rows are rebased.
fn visible_start(selected: usize, count: usize, height: u16) -> usize {
    let visible = usize::from(height);
    selected
        .saturating_add(1)
        .saturating_sub(visible)
        .min(count.saturating_sub(visible))
}

fn render_theme_list(frame: &mut Frame<'_>, area: Rect, app: &TuiApp, hits: &mut HitMap) {
    let theme = app.theme;
    let selected = app.theme_cursor();
    let block = panel(theme, "Themes", VisualState::Focused)
        .padding(Padding::horizontal(1))
        .title_bottom(Line::from(Span::styled(
            format!(" {} / {} ", selected + 1, theme::ALL.len()),
            theme.muted_style(),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let start = visible_start(selected, theme::ALL.len(), inner.height);

    for (row, (index, candidate)) in theme::ALL
        .iter()
        .enumerate()
        .skip(start)
        .take(usize::from(inner.height))
        .enumerate()
    {
        let is_selected = index == selected;
        let style = if is_selected {
            theme.selected_style()
        } else {
            theme.surface_style()
        };
        let swatch = if theme.nerd_icons { "█" } else { "#" };
        let mut spans = vec![Span::styled(if is_selected { "> " } else { "  " }, style)];
        // Never sacrifice a name for decoration on tiny terminals.
        if inner.width >= 24 {
            for color in [
                candidate.accent,
                candidate.success,
                candidate.warning,
                candidate.danger,
            ] {
                spans.push(Span::styled(swatch, Style::default().fg(color)));
            }
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(candidate.label, style));
        let rect = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
        frame.render_widget(Paragraph::new(Line::from(spans)).style(style), rect);
    }
    // push_rows indexes the visible window; translate back to the model index.
    hits.push_rows(inner, theme::ALL.len().saturating_sub(start), |row| {
        HitTarget::ThemeRow(start + row)
    });
}

/// A small dashboard specimen rather than a palette of unexplained colours.
fn render_theme_preview(frame: &mut Frame<'_>, area: Rect, theme: Theme) {
    let block = panel(
        theme,
        &format!("Preview: {}", theme.label),
        VisualState::Normal,
    )
    .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = vec![
        section(theme, "AirGradient", "AIR MONITOR"),
        Line::raw(""),
        Line::from(vec![
            Span::styled("AQI 42  ", theme.value_style()),
            badge(theme, "Good", VisualState::Success),
        ]),
        meter(
            theme,
            42.0 / 500.0,
            inner.width.min(32),
            VisualState::Success,
        ),
        Line::from(Span::styled(
            "Latest air quality reading.",
            theme.muted_style(),
        )),
        Line::raw(""),
        section(theme, "STATUS", "Colour with meaning"),
        Line::from(vec![
            badge(theme, "Good", VisualState::Success),
            Span::raw(" "),
            badge(theme, "Moderate", VisualState::Warning),
        ]),
        Line::from(vec![
            badge(theme, "Unhealthy", VisualState::Error),
            Span::raw(" "),
            badge(theme, "Unknown", VisualState::Disabled),
        ]),
        Line::raw(""),
        section(theme, "INTERACTION", "Focus and selection"),
        Line::from(Span::styled(" > selected row ", theme.selected_style())),
        Line::from(vec![
            Span::styled("label text  ", theme.label_style()),
            Span::styled("value text", theme.value_style()),
        ]),
        Line::from(Span::styled("muted text", theme.muted_style())),
        Line::raw(""),
        Line::from(key_hint(theme, "Enter", "apply this theme")),
        Line::from(Span::styled(
            "Esc restores your previous theme.",
            theme.muted_style(),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).style(theme.surface_style()), inner);
}

pub(super) fn render_config_editor(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &TuiApp,
    hits: &mut HitMap,
) {
    let theme = app.theme;
    let area = screen_inset(area);
    let outer = panel(theme, "Config", VisualState::Focused).padding(Padding::horizontal(1));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);
    let [heading, content, controls] = settings_regions(inner);
    frame.render_widget(
        Paragraph::new(section(theme, "PREFERENCES", "Unsaved draft")),
        heading,
    );
    if app.config_editor_editing.is_some() {
        let mut keys = key_hint(theme, "Enter", "confirm");
        keys.push(Span::raw("   "));
        keys.extend(key_hint(theme, "Esc", "cancel edit"));
        frame.render_widget(Paragraph::new(Line::from(keys)), controls);
    } else {
        render_controls(frame, controls, theme, "navigate", "edit/toggle");
    }

    let form = if content.width >= 90 {
        let [form, help] = Layout::horizontal([Constraint::Min(54), Constraint::Length(32)])
            .spacing(1)
            .areas(content);
        render_config_help(frame, help, app);
        form
    } else {
        content
    };
    let error_height = if app.config_editor_error.is_some() && form.height >= 5 {
        3
    } else {
        0
    };
    let [fields, error] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(error_height)]).areas(form);
    render_config_fields(frame, fields, app, hits);
    if let Some(message) = &app.config_editor_error {
        frame.render_widget(
            Paragraph::new(format!("error: {message}"))
                .style(theme.state_style(VisualState::Error))
                .wrap(Wrap { trim: true }),
            error,
        );
    }
}

fn render_config_fields(frame: &mut Frame<'_>, area: Rect, app: &TuiApp, hits: &mut HitMap) {
    let stacked = area.width < 48;
    let row_height = if stacked || area.height >= (ConfigField::ALL.len() * 2) as u16 {
        2
    } else {
        1
    };
    let visible = area.height / row_height;
    let start = visible_start(app.config_field_cursor(), ConfigField::ALL.len(), visible);
    for (row, (index, field)) in ConfigField::ALL
        .iter()
        .enumerate()
        .skip(start)
        .take(usize::from(visible))
        .enumerate()
    {
        let selected = index == app.config_field_cursor();
        let editing = selected && app.config_editor_editing.is_some();
        let style = if selected {
            app.theme.selected_style()
        } else {
            app.theme.raised_style()
        };
        let label_style = if selected {
            style
        } else {
            app.theme.label_style()
        };
        let value_style = if selected {
            style
        } else {
            app.theme.value_style()
        };
        let label = config_field_label(*field);
        let value = config_field_display_value(app, *field, editing);
        let cursor = Span::styled(if selected { "> " } else { "  " }, style);
        let lines = if stacked {
            vec![
                Line::from(vec![cursor, Span::styled(label, label_style)]),
                Line::from(Span::styled(value, value_style)),
            ]
        } else {
            vec![Line::from(vec![
                cursor,
                Span::styled(format!("{label:<CONFIG_LABEL_WIDTH$}"), label_style),
                Span::styled(value, value_style),
            ])]
        };
        let rect = Rect::new(
            area.x,
            area.y + row as u16 * row_height,
            area.width,
            row_height,
        );
        // Values never wrap: click rectangles are exactly the rendered rows.
        let painted = Rect {
            height: if stacked { row_height } else { 1 },
            ..rect
        };
        frame.render_widget(Paragraph::new(lines).style(style), painted);
        if editing {
            let offset = if stacked {
                0
            } else {
                CONFIG_LABEL_WIDTH as u16 + 2
            };
            let value_area = Rect::new(
                painted.x + offset,
                painted.y + u16::from(stacked),
                painted.width.saturating_sub(offset),
                1,
            );
            render_input(
                frame,
                value_area,
                Line::from(config_field_display_value(app, *field, true)),
                value_style,
            );
        }
        hits.push(painted, HitTarget::ConfigRow(index));
    }
}

fn render_config_help(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let theme = app.theme;
    let block = panel(theme, "Field guide", VisualState::Normal).padding(Padding::horizontal(1));
    let text = match app.config_editor_field() {
        ConfigField::ServerUrl => "Device address on your network. Include http:// or https://.",
        ConfigField::RefreshInterval => {
            "Seconds between automatic readings. The current draft does not change the running monitor until saved."
        }
        ConfigField::NotificationsEnabled => {
            "Enable or disable desktop notifications in the saved configuration."
        }
        ConfigField::StartMinimized => "Choose whether the desktop app starts minimized.",
        ConfigField::Theme => {
            "Open the theme gallery. Preview colours live, then confirm your choice."
        }
        ConfigField::SaveAndClose => {
            "Validate and save this draft, then return to the monitor. Esc discards unsaved changes."
        }
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(badge(
                theme,
                if app.config_editor_editing.is_some() {
                    "EDITING"
                } else {
                    "DRAFT"
                },
                VisualState::Info,
            )),
            Line::raw(""),
            Line::from(Span::styled(
                config_field_label(app.config_editor_field()),
                theme.value_style(),
            )),
            Line::raw(""),
            Line::from(text),
            Line::raw(""),
            Line::from(Span::styled(
                "Changes stay local until Save & Close.",
                theme.muted_style(),
            )),
        ])
        .style(theme.surface_style())
        .wrap(Wrap { trim: true })
        .block(block),
        area,
    );
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

/// Editing values remain model-owned; the appended cursor is presentation only.
fn config_field_display_value(app: &TuiApp, field: ConfigField, editing: bool) -> String {
    if editing {
        let buffer = app.config_editor_editing.as_deref().unwrap_or("");
        let cursor = if app.theme.nerd_icons { "█" } else { "_" };
        return format!("{buffer}{cursor}");
    }
    match field {
        ConfigField::ServerUrl => app
            .config_draft
            .server_url
            .clone()
            .unwrap_or_else(|| "(none)".to_string()),
        ConfigField::RefreshInterval => app.config_draft.refresh_interval.to_string(),
        ConfigField::NotificationsEnabled => {
            bool_label(app.config_draft.notifications_enabled).to_string()
        }
        ConfigField::StartMinimized => bool_label(app.config_draft.start_minimized).to_string(),
        ConfigField::Theme => Theme::by_id(&app.config_draft.theme).label.to_string(),
        ConfigField::SaveAndClose => String::new(),
    }
}

fn bool_label(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}
