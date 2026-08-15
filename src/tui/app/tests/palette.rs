//! The `:` command palette: each verb it accepts, what it persists, and the
//! line-editing keys.

use super::*;

#[test]
fn palette_submit_sets_url_and_persists() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_with_config(temp_dir.path());

    app.open_command_palette();
    for c in "url 192.168.1.201".chars() {
        app.palette_push_char(c);
    }
    let outcome = app.palette_submit();

    assert_eq!(outcome, PaletteOutcome::Continue);
    assert_eq!(
        app.configured_url.as_ref().map(|u| u.to_string()),
        Some("http://192.168.1.201/".to_string())
    );
    assert_eq!(
        config::read_config(&path).unwrap().server_url.as_deref(),
        Some("http://192.168.1.201/")
    );
}

#[test]
fn palette_submit_rejects_out_of_range_refresh_interval() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.open_command_palette();
    for c in "refresh 1".chars() {
        app.palette_push_char(c);
    }

    app.palette_submit();

    assert_eq!(app.refresh_interval, Duration::from_secs(30));
    assert_eq!(
        app.palette_message.as_ref().map(|(_, is_error)| *is_error),
        Some(true)
    );
}

#[test]
fn palette_submit_quit_returns_quit_outcome() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.open_command_palette();
    app.palette_push_char('q');

    assert_eq!(app.palette_submit(), PaletteOutcome::Quit);
}

#[test]
fn palette_submit_unknown_command_shows_error_and_stays() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.open_command_palette();
    for c in "bogus".chars() {
        app.palette_push_char(c);
    }

    let outcome = app.palette_submit();

    assert_eq!(outcome, PaletteOutcome::Continue);
    assert_eq!(
        app.palette_message.as_ref().map(|(_, is_error)| *is_error),
        Some(true)
    );
}

#[test]
fn ctrl_w_deletes_the_word_before_the_cursor() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.open_command_palette();
    for c in "url 192.168.1.201".chars() {
        app.palette_push_char(c);
    }

    app.palette_delete_word();
    assert_eq!(app.palette_input, "url ");

    app.palette_delete_word();
    assert_eq!(app.palette_input, "");

    // Deleting from an empty line is a no-op, not a panic.
    app.palette_delete_word();
    assert_eq!(app.palette_input, "");
}

#[test]
fn ctrl_u_clears_the_whole_palette_line() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.open_command_palette();
    for c in "theme nord".chars() {
        app.palette_push_char(c);
    }

    app.palette_clear_line();

    assert_eq!(app.palette_input, "");
}

// -- Leader / which-key -------------------------------------------------

#[test]
fn palette_rejects_an_unknown_theme_without_touching_the_stored_one() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_with_config(temp_dir.path());
    let stored = theme::ALL[1];
    config::set_theme(&path, stored.id).unwrap();
    app.theme = stored;

    app.open_command_palette();
    for c in "theme drakula".chars() {
        app.palette_push_char(c);
    }
    app.palette_submit();

    let (message, is_error) = app.palette_message.clone().expect("message should be set");
    assert!(message.contains("unknown theme: drakula"), "{message}");
    assert!(is_error);
    assert_eq!(app.theme.id, stored.id);
    assert_eq!(config::read_config(&path).unwrap().theme, stored.id);
}

#[test]
fn palette_save_writes_the_live_session_to_the_config_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_with_config(temp_dir.path());
    app.configured_url = Some(Url::parse("http://192.168.1.77/").unwrap());
    app.refresh_interval = Duration::from_secs(45);
    app.theme = theme::ALL[1];

    app.open_command_palette();
    for c in "save".chars() {
        app.palette_push_char(c);
    }
    app.palette_submit();

    let (message, is_error) = app.palette_message.clone().expect("message should be set");
    assert!(!is_error, "{message}");
    let saved = config::read_config(&path).unwrap();
    assert_eq!(saved.server_url.as_deref(), Some("http://192.168.1.77/"));
    assert_eq!(saved.refresh_interval_secs, 45);
    assert_eq!(saved.theme, theme::ALL[1].id);
}

#[test]
fn palette_save_reports_when_there_is_no_config_file() {
    let mut app = app_with_refresh(Duration::from_secs(30));

    app.open_command_palette();
    for c in "save".chars() {
        app.palette_push_char(c);
    }
    app.palette_submit();

    let (message, is_error) = app.palette_message.clone().expect("message should be set");
    assert!(is_error, "{message}");
    assert!(message.contains("no config file"), "{message}");
}
