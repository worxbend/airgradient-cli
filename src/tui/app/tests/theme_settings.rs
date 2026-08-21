//! The theme picker: live preview, cancel, confirm, and the motions and clicks
//! that move its cursor.

use super::*;

#[test]
fn theme_settings_preview_reverts_on_close_without_confirm() {
    let mut app = app_with_refresh(refresh(30));
    let original = app.theme;

    app.open_theme_settings();
    app.theme_cursor_down();
    assert_ne!(app.theme.id, original.id);

    app.close_theme_settings();
    assert_eq!(app.theme.id, original.id);
    assert_eq!(app.view, View::Dashboard);
}

#[test]
fn theme_settings_confirm_applies_and_persists() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_with_config(temp_dir.path());

    app.open_theme_settings();
    app.theme_cursor_down();
    let chosen = app.theme;
    app.confirm_theme_settings();

    assert_eq!(app.theme.id, chosen.id);
    assert_eq!(app.view, View::Dashboard);
    assert_eq!(
        config::read_config(&path).unwrap().theme,
        chosen.id.to_string()
    );
}

#[test]
fn theme_motions_jump_to_the_ends_of_the_list() {
    let mut app = app_with_refresh(refresh(30));
    app.open_theme_settings();

    app.theme_cursor_last();
    assert_eq!(app.theme_cursor(), theme::ALL.len() - 1);
    assert_eq!(app.theme, theme::ALL[theme::ALL.len() - 1]);

    app.theme_cursor_first();
    assert_eq!(app.theme_cursor(), 0);
    assert_eq!(app.theme, theme::ALL[0]);
}

#[test]
fn theme_half_page_motions_stay_inside_the_list() {
    let mut app = app_with_refresh(refresh(30));
    app.open_theme_settings();
    app.theme_cursor_first();

    app.theme_cursor_half_page(1);
    assert_eq!(app.theme_cursor(), theme::ALL.len() / 2);

    // Repeated scrolling clamps at the end rather than wrapping or panicking.
    for _ in 0..5 {
        app.theme_cursor_half_page(1);
    }
    assert_eq!(app.theme_cursor(), theme::ALL.len() - 1);

    for _ in 0..5 {
        app.theme_cursor_half_page(-1);
    }
    assert_eq!(app.theme_cursor(), 0);
}

#[test]
fn clicking_a_theme_row_selects_and_previews_it() {
    let mut app = app_with_refresh(refresh(30));
    app.open_theme_settings();

    app.select_theme_index(3);

    assert_eq!(app.theme_cursor(), 3);
    assert_eq!(app.theme, theme::ALL[3]);
}

#[test]
fn selecting_out_of_range_clamps_instead_of_panicking() {
    let mut app = app_with_refresh(refresh(30));
    app.open_theme_settings();

    app.select_theme_index(usize::MAX);

    assert_eq!(app.theme_cursor(), theme::ALL.len() - 1);
}
