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

// -- Tolerant loading ---------------------------------------------------

#[test]
fn a_malformed_field_does_not_reset_the_settings_next_to_it() {
    // The editor used to load the draft with the strict reader, so one bad
    // value made it fall back to `Config::default()` for *every* field —
    // and Save & Close then wrote those defaults over the desktop app's real
    // values. Only the malformed field may fall back.
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("config.json");
    std::fs::write(
        &path,
        r#"{
            "server_url": "http://192.168.1.7/",
            "refresh_interval_secs": "not a number",
            "notifications_enabled": false,
            "start_minimized": true,
            "desktop_only_key": "kept"
        }"#,
    )
    .unwrap();

    let mut app = app_with_refresh(Duration::from_secs(30));
    app.config_path = Some(path.clone());
    app.open_config_editor();

    // The two booleans the CLI never edits survived the malformed neighbour.
    assert!(!app.config_draft.notifications_enabled);
    assert!(app.config_draft.start_minimized);
    assert_eq!(
        app.config_draft.server_url.as_deref(),
        Some("http://192.168.1.7/")
    );
    // Only the unparseable field fell back, and the user is told why.
    assert_eq!(
        app.config_draft.refresh_interval_secs,
        config::DEFAULT_REFRESH_INTERVAL_SECS
    );
    assert!(
        app.config_editor_error
            .as_deref()
            .is_some_and(|error| error.contains("refresh_interval_secs")),
        "{:?}",
        app.config_editor_error
    );

    app.config_editor_nav_last();
    app.config_editor_confirm(); // Save & Close

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(written["notifications_enabled"], false);
    assert_eq!(written["start_minimized"], true);
    assert_eq!(written["desktop_only_key"], "kept");
}

// -- Rejected edits -----------------------------------------------------

#[test]
fn a_rejected_value_stays_in_the_buffer_to_be_corrected() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.open_config_editor();
    app.config_editor_confirm(); // begin editing the server URL

    for c in "ftp://192.168.1.4".chars() {
        app.config_editor_push_char(c);
    }
    app.config_editor_confirm(); // rejected: unsupported scheme

    assert_eq!(
        app.config_editor_editing.as_deref(),
        Some("ftp://192.168.1.4"),
        "a rejected value must not be thrown away"
    );
    assert!(app.config_editor_error.is_some());
    assert_eq!(app.config_draft.server_url, None);
}

#[test]
fn a_rejected_interval_stays_in_the_buffer_and_an_accepted_one_clears_it() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.open_config_editor();
    app.config_editor_nav_down();
    assert_eq!(app.config_editor_field(), ConfigField::RefreshInterval);

    app.config_editor_confirm(); // seeds the buffer with "30"
    app.config_editor_clear_line();
    for c in "99999".chars() {
        app.config_editor_push_char(c);
    }
    app.config_editor_confirm(); // rejected: out of range

    assert_eq!(app.config_editor_editing.as_deref(), Some("99999"));
    assert!(app.config_editor_error.is_some());

    app.config_editor_clear_line();
    for c in "45".chars() {
        app.config_editor_push_char(c);
    }
    app.config_editor_confirm(); // accepted

    assert_eq!(app.config_editor_editing, None);
    assert_eq!(app.config_editor_error, None);
    assert_eq!(app.config_draft.refresh_interval_secs, 45);
}

#[test]
fn clearing_the_url_field_is_accepted_rather_than_rejected() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.open_config_editor();
    app.config_draft.server_url = Some("http://192.168.1.4/".to_string());

    app.config_editor_confirm(); // begin editing
    app.config_editor_clear_line();
    app.config_editor_confirm(); // an empty URL means "no device configured"

    assert_eq!(app.config_editor_editing, None);
    assert_eq!(app.config_editor_error, None);
    assert_eq!(app.config_draft.server_url, None);
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
