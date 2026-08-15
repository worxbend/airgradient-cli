//! The config editor: the draft it accumulates, what Save & Close writes, and
//! the motions and clicks that move between its fields.

use super::*;

#[test]
fn config_editor_save_persists_draft_and_applies_live_state() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_with_config(temp_dir.path());

    app.open_config_editor();
    assert_eq!(app.config_editor_field(), ConfigField::ServerUrl);

    app.config_editor_confirm(); // begin editing server url
    for c in "192.168.1.50".chars() {
        app.config_editor_push_char(c);
    }
    app.config_editor_confirm(); // commit

    app.config_editor_nav_down();
    assert_eq!(app.config_editor_field(), ConfigField::RefreshInterval);
    app.config_editor_confirm(); // seeds the buffer with the current "30"
    app.config_editor_backspace();
    app.config_editor_backspace();
    for c in "45".chars() {
        app.config_editor_push_char(c);
    }
    app.config_editor_confirm();

    for _ in 0..(ConfigField::ALL.len() - 2) {
        app.config_editor_nav_down();
    }
    assert_eq!(app.config_editor_field(), ConfigField::SaveAndClose);
    app.config_editor_confirm();

    assert_eq!(app.view, View::Dashboard);
    assert_eq!(
        app.configured_url.as_ref().map(|u| u.to_string()),
        Some("http://192.168.1.50/".to_string())
    );
    assert_eq!(app.refresh_interval, Duration::from_secs(45));

    let persisted = config::read_config(&path).unwrap();
    assert_eq!(
        persisted.server_url.as_deref(),
        Some("http://192.168.1.50/")
    );
    assert_eq!(persisted.refresh_interval_secs, 45);
}

#[test]
fn config_editor_esc_discards_draft() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_with_config(temp_dir.path());
    config::set_refresh_interval(&path, 90).unwrap();

    app.open_config_editor();
    app.config_editor_confirm(); // begin editing server url
    for c in "192.168.1.99".chars() {
        app.config_editor_push_char(c);
    }
    app.config_editor_cancel_edit();
    app.close_config_editor();

    assert_eq!(app.view, View::Dashboard);
    assert_eq!(app.configured_url, None);
    assert_eq!(
        config::read_config(&path).unwrap().refresh_interval_secs,
        90
    );
}

#[test]
fn config_editor_toggles_booleans_immediately() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.open_config_editor();
    app.config_editor_nav_down();
    app.config_editor_nav_down();
    assert_eq!(app.config_editor_field(), ConfigField::NotificationsEnabled);

    let before = app.config_draft.notifications_enabled;
    app.config_editor_confirm();
    assert_eq!(app.config_draft.notifications_enabled, !before);
}

// -- Vim motions --------------------------------------------------------

#[test]
fn config_editor_motions_jump_to_the_ends_of_the_field_list() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.open_config_editor();

    app.config_editor_nav_last();
    assert_eq!(app.config_editor_field(), ConfigField::SaveAndClose);

    app.config_editor_nav_first();
    assert_eq!(app.config_editor_field(), ConfigField::ServerUrl);
}

// -- Mouse selection ----------------------------------------------------

#[test]
fn clicking_a_config_row_is_ignored_while_a_field_is_being_edited() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.open_config_editor();
    app.config_editor_confirm(); // start editing the server URL
    assert!(app.config_editor_editing.is_some());

    app.select_config_field_index(4);

    // The click must not retarget the buffer the user is typing into.
    assert_eq!(app.config_editor_field(), ConfigField::ServerUrl);
}

// -- Text-entry chords --------------------------------------------------

#[test]
fn theme_chosen_inside_the_config_editor_survives_save_and_close() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_with_config(temp_dir.path());
    config::set_theme(&path, theme::ALL[0].id).unwrap();
    let chosen = theme::ALL[1];

    app.open_config_editor();
    for _ in 0..ConfigField::ALL.len() {
        if app.config_editor_field() == ConfigField::Theme {
            break;
        }
        app.config_editor_nav_down();
    }
    assert_eq!(app.config_editor_field(), ConfigField::Theme);
    app.config_editor_confirm(); // opens the picker from inside the editor
    assert_eq!(app.view, View::ThemeSettings);
    app.select_theme_index(1);
    app.confirm_theme_settings();
    assert_eq!(app.view, View::ConfigEditor);

    app.config_editor_nav_last();
    assert_eq!(app.config_editor_field(), ConfigField::SaveAndClose);
    app.config_editor_confirm();

    assert_eq!(app.theme.id, chosen.id);
    assert_eq!(config::read_config(&path).unwrap().theme, chosen.id);
}
