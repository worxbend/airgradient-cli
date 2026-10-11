//! Render-driven scrolling and background contracts for the settings/history surfaces.

use std::time::{Duration, SystemTime};

use airgradient_cli::{
    history::Reading,
    refresh_interval::RefreshInterval,
    sensors::SensorSnapshot,
    tui::{app::TuiApp, theme, ui},
};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

fn app() -> TuiApp {
    TuiApp::new(None, RefreshInterval::clamping(Duration::from_secs(30)))
}

fn render(app: &TuiApp, width: u16, height: u16) -> (Buffer, ui::HitMap) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut hits = ui::HitMap::default();
    terminal
        .draw(|frame| ui::draw_with_hits(frame, app, &mut hits))
        .unwrap();
    (terminal.backend().buffer().clone(), hits)
}

fn row_text(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn target_cell(buffer: &Buffer, hits: &ui::HitMap, target: ui::HitTarget) -> Option<(u16, u16)> {
    (0..buffer.area.height).find_map(|y| {
        (0..buffer.area.width)
            .find(|&x| hits.hit(x, y) == Some(target))
            .map(|x| (x, y))
    })
}

#[test]
fn every_theme_scrolls_into_view_with_its_absolute_click_target() {
    let mut app = app();
    app.open_theme_settings();
    for (width, height) in [(36, 20), (60, 12), (100, 10), (100, 40)] {
        for (selected, candidate) in theme::ALL.iter().enumerate() {
            app.select_theme_index(selected);
            let (buffer, hits) = render(&app, width, height);
            let (x, y) = target_cell(&buffer, &hits, ui::HitTarget::ThemeRow(selected))
                .unwrap_or_else(|| panic!("{} not reachable at {width}x{height}", candidate.id));
            assert!(row_text(&buffer, y).contains(candidate.label));
            assert_eq!(buffer[(x, y)].bg, app.theme.selected_style().bg.unwrap());
            for (index, candidate) in theme::ALL.iter().enumerate() {
                if let Some((_, y)) = target_cell(&buffer, &hits, ui::HitTarget::ThemeRow(index)) {
                    assert!(
                        row_text(&buffer, y).contains(candidate.label),
                        "wrong scrolled target {index}"
                    );
                }
            }
        }
    }
}

#[test]
fn the_full_theme_gallery_fits_the_standard_terminal() {
    let mut app = app();
    app.open_theme_settings();
    for selected in 0..theme::ALL.len() {
        app.select_theme_index(selected);
        let (buffer, hits) = render(&app, 100, 40);
        for (index, candidate) in theme::ALL.iter().enumerate() {
            let y = (0..40)
                .find(|&y| hits.hit(8, y) == Some(ui::HitTarget::ThemeRow(index)))
                .unwrap();
            assert!(row_text(&buffer, y).contains(candidate.label));
        }
    }
}

#[test]
fn compact_config_keeps_values_and_scrolls_each_selected_field_into_view() {
    let mut app = app();
    app.open_config_editor();
    let labels = [
        "Server URL",
        "Refresh interval",
        "Notifications enabled",
        "Start minimized",
        "Theme",
        "[ Save & Close ]",
    ];
    for (width, height) in [(36, 20), (36, 10), (80, 12), (100, 40)] {
        for (index, label) in labels.iter().enumerate() {
            app.select_config_field_index(index);
            let (buffer, hits) = render(&app, width, height);
            let (x, y) = target_cell(&buffer, &hits, ui::HitTarget::ConfigRow(index)).unwrap();
            assert!(row_text(&buffer, y).contains(label));
            if width == 36 && index == 1 {
                assert!(row_text(&buffer, y + 1).contains("30"));
                assert_eq!(hits.hit(x, y + 1), Some(ui::HitTarget::ConfigRow(index)));
            }
        }
    }
}

fn with_history() -> TuiApp {
    let mut app = app();
    let now = SystemTime::now();
    app.restore_history(
        (0..20)
            .map(|index| {
                Reading::new(
                    now - Duration::from_secs((20 - index) * 30),
                    SensorSnapshot {
                        aqi: Some(30.0 + index as f64),
                        ..SensorSnapshot::default()
                    },
                )
            })
            .collect::<Vec<_>>(),
    );
    app.current_snapshot = Some(SensorSnapshot {
        aqi: Some(42.0),
        ..SensorSnapshot::default()
    });
    app.open_history();
    app
}

#[test]
fn short_history_list_keeps_the_selected_metric_clickable() {
    let mut app = with_history();
    let labels = app
        .metrics()
        .iter()
        .map(|metric| metric.label)
        .collect::<Vec<_>>();
    for (index, label) in labels.iter().enumerate() {
        let (buffer, hits) = render(&app, 100, 12);
        let (_, y) = target_cell(&buffer, &hits, ui::HitTarget::HistoryRow(index)).unwrap();
        assert!(row_text(&buffer, y).contains(label));
        app.history_next_metric();
    }
}

#[test]
fn telemetry_summary_uses_real_recorded_range_and_count() {
    let app = with_history();
    let (buffer, _) = render(&app, 100, 40);
    let text = (0..40)
        .map(|y| row_text(&buffer, y))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Low 30   High 49"), "{text}");
    assert!(text.contains("20 readings"), "{text}");
    assert!(text.contains("Now  42"), "{text}");
}

#[test]
fn history_canvas_matches_its_panel_in_every_theme() {
    let mut app = with_history();
    for candidate in theme::ALL {
        app.theme = *candidate;
        let (buffer, _) = render(&app, 100, 40);
        let mut plotted = 0;
        for cell in &buffer.content {
            assert!(
                candidate.is_background(cell.bg),
                "{} unexpected fill {:?}",
                candidate.id,
                cell.bg
            );
            if cell
                .symbol()
                .chars()
                .any(|ch| ('\u{2800}'..='\u{28ff}').contains(&ch))
            {
                plotted += 1;
                assert_eq!(
                    cell.bg,
                    candidate.surface(),
                    "canvas hole in {}",
                    candidate.id
                );
            }
        }
        if candidate.nerd_icons {
            assert!(plotted > 0, "{} lost its plot", candidate.id);
        } else {
            assert_eq!(plotted, 0, "TTY-safe charts must not draw braille");
        }
    }
}

#[test]
fn settings_and_history_survive_tiny_resize_frames() {
    let mut app = with_history();
    for (width, height) in [(1, 1), (8, 3), (24, 8)] {
        render(&app, width, height);
        app.open_theme_settings();
        app.theme_cursor_last();
        render(&app, width, height);
        app.open_config_editor();
        app.config_editor_nav_last();
        render(&app, width, height);
        app.open_history();
    }
}

#[test]
fn editing_a_long_value_keeps_its_tail_cursor_label_and_click_target() {
    let mut app = app();
    app.open_config_editor();
    app.config_editor_editing = Some(format!("{}TAIL", "界e\u{301}".repeat(60)));
    for (width, height) in [(36, 20), (80, 24), (120, 40)] {
        let (buffer, hits) = render(&app, width, height);
        let (_, y) = target_cell(&buffer, &hits, ui::HitTarget::ConfigRow(0)).unwrap();
        assert!(row_text(&buffer, y).contains("Server URL"));
        let row = y + u16::from(width == 36);
        assert!(
            row_text(&buffer, row).contains("TAIL█"),
            "{}",
            row_text(&buffer, row)
        );
    }
}
