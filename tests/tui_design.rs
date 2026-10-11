//! Render-buffer contracts, shared with the headless synthetic preview gallery.
#[allow(dead_code)]
#[path = "../examples/tui_preview.rs"]
mod preview;

use std::time::Duration;

use airgradient_cli::tui::{
    app::{PaletteMessageKind, PaletteOutcome, View},
    theme, ui,
};
use ratatui::{buffer::Buffer, style::Modifier};

fn assert_backgrounds(buffer: &Buffer, candidate: theme::Theme, scene: &str) {
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            assert!(
                candidate.is_background(cell.bg),
                "{} / {scene} / {}x{} / ({x},{y}) {:?}: unapproved background {:?}",
                candidate.id,
                buffer.area.width,
                buffer.area.height,
                cell.symbol(),
                cell.bg
            );
        }
    }
}

#[test]
fn every_theme_view_and_overlay_uses_only_semantic_backgrounds() {
    for &candidate in theme::ALL {
        for scene in preview::SCENES {
            for (width, height) in [(36, 20), (60, 24), (100, 40), (144, 46)] {
                let app = preview::fixture(candidate, scene);
                let (buffer, _) = preview::render(&app, width, height).unwrap();
                assert_backgrounds(&buffer, candidate, scene);
            }
        }
        // Alerts must be opaque and theme-safe above every full-screen view too.
        for view in [
            View::ThemeSettings,
            View::ConfigEditor,
            View::History,
            View::CommandPalette,
        ] {
            let mut app = preview::fixture(candidate, "alerts");
            app.view = view;
            for (width, height) in [(36, 20), (100, 40)] {
                let (buffer, _) = preview::render(&app, width, height).unwrap();
                assert_backgrounds(&buffer, candidate, "alert over full-screen view");
            }
        }
    }
}

#[test]
fn zero_and_tiny_resize_frames_are_safe_in_every_theme_and_scene() {
    for &candidate in theme::ALL {
        for scene in preview::SCENES {
            let app = preview::fixture(candidate, scene);
            for (width, height) in [(0, 0), (0, 10), (10, 0), (1, 1), (8, 3), (24, 8)] {
                let (buffer, hits) = preview::render(&app, width, height).unwrap();
                assert_eq!(
                    buffer.content.len(),
                    usize::from(width) * usize::from(height)
                );
                assert_backgrounds(&buffer, candidate, scene);
                assert_eq!(hits.hit(width, height), None, "out-of-bounds hit");
            }
        }
    }
}

#[test]
fn selected_last_theme_is_visible_and_clickable_at_its_rendered_label() {
    for &origin in theme::ALL {
        for (width, height) in [(36, 20), (60, 12), (100, 10), (144, 46)] {
            let mut app = preview::fixture(origin, "themes");
            app.theme_cursor_last();
            let last = theme::ALL.len() - 1;
            let candidate = theme::ALL[last];
            let (buffer, hits) = preview::render(&app, width, height).unwrap();
            let mut found = false;
            for y in 0..height {
                for x in 0..width {
                    // Search visible glyphs first: a stale hit zone alone is not proof.
                    let suffix: String = (x..width)
                        .map(|column| buffer[(column, y)].symbol())
                        .collect();
                    if suffix.starts_with(candidate.label)
                        && hits.hit(x, y) == Some(ui::HitTarget::ThemeRow(last))
                    {
                        assert_eq!(hits.hit(x, y), Some(ui::HitTarget::ThemeRow(last)));
                        assert_eq!(buffer[(x, y)].bg, candidate.selected_style().bg.unwrap());
                        assert!(buffer[(x, y)].modifier.contains(Modifier::BOLD));
                        found = true;
                    }
                }
            }
            assert!(found, "last theme absent at {width}x{height}");
            assert_eq!(app.theme_cursor(), last);
            assert!(app.config_path.is_none());
        }
    }
}

#[test]
fn long_unicode_palette_keeps_tail_and_cursor_visible() {
    for &candidate in theme::ALL {
        for (width, height) in [(36, 20), (60, 24), (144, 46)] {
            let mut app = preview::fixture(candidate, "palette");
            app.palette_input = format!("url {}TAIL", "界e\u{301}🧪".repeat(64));
            let (buffer, _) = preview::render(&app, width, height).unwrap();
            let cursor = if candidate.nerd_icons { "▎" } else { "_" };
            let expected = format!("TAIL{cursor}");
            assert!(
                preview::text(&buffer).contains(&expected),
                "{} lost Unicode tail/cursor at {width}x{height}:\n{}",
                candidate.id,
                preview::text(&buffer)
            );
        }
    }
}

#[test]
fn submitted_commands_show_success_and_error_feedback_without_config_io() {
    for &candidate in theme::ALL {
        for (command, kind, color) in [
            ("refresh 60", PaletteMessageKind::Success, candidate.success),
            ("save", PaletteMessageKind::Error, candidate.danger),
        ] {
            let mut app = preview::fixture(candidate, "palette");
            assert!(app.config_path.is_none() && app.history_path.is_none());
            app.palette_input = command.into();
            assert_eq!(app.palette_submit(), PaletteOutcome::Continue);
            assert_eq!(app.view, View::Dashboard);
            let message = app.palette_message.as_ref().unwrap();
            assert_eq!(message.kind, kind);
            for (width, height) in [(60, 24), (100, 40), (144, 46)] {
                let (buffer, _) = preview::render(&app, width, height).unwrap();
                assert!(
                    preview::text(&buffer).contains(&message.text),
                    "feedback missing:\n{}",
                    preview::text(&buffer)
                );
                assert!(
                    buffer
                        .content
                        .iter()
                        .any(|cell| cell.fg == color
                            && cell.symbol().chars().any(char::is_alphabetic))
                );
            }
            assert!(app.config_path.is_none() && app.history_path.is_none());
        }
    }
}

#[test]
fn synthetic_preview_buffers_do_not_drift_with_wall_clock() {
    let candidate = theme::Theme::default_theme();
    let before: Vec<_> = preview::SCENES
        .iter()
        .map(|scene| {
            let app = preview::fixture(candidate, scene);
            (scene, preview::render(&app, 144, 46).unwrap().0)
        })
        .collect();
    let history = &before
        .iter()
        .find(|(scene, _)| **scene == "history")
        .unwrap()
        .1;
    assert!(
        preview::text(history).contains("23m 30s ago"),
        "history must use the fixed acquisition clock, not wall-clock age"
    );
    // Cross a clock tick: rendering the same frame twice immediately misses time bugs.
    std::thread::sleep(Duration::from_millis(1100));
    for (scene, expected) in before {
        let app = preview::fixture(candidate, scene);
        let actual = preview::render(&app, 144, 46).unwrap().0;
        assert_eq!(actual, expected, "{scene} depends on wall-clock time");
    }
}

#[test]
fn html_export_preserves_cell_styles_and_escapes_fixture_text() {
    use ratatui::{layout::Rect, style::Color};
    let mut buffer = Buffer::empty(Rect::new(0, 0, 2, 1));
    buffer[(0, 0)]
        .set_symbol("<")
        .set_fg(Color::Rgb(1, 2, 3))
        .set_bg(Color::Rgb(4, 5, 6));
    buffer[(0, 0)].modifier = Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED;
    buffer[(1, 0)].set_symbol("&");
    let html = preview::buffer_html(&buffer);
    for expected in [
        "&lt;",
        "&amp;",
        "#010203",
        "#040506",
        "font-weight:bold",
        "font-style:italic",
        "text-decoration:underline",
        "data-bg=\"Reset\"",
        "var(--terminal-bg)",
    ] {
        assert!(html.contains(expected), "HTML lost {expected}");
    }
}
