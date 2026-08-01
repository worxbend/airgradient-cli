//! Tests for config defaults, path precedence, refresh bounds, the
//! read/write round trip, and tolerant parsing.

use std::fs;

use super::*;

#[test]
fn default_values_match_desktop_contract() {
    let config = Config::default();

    assert_eq!(config.server_url, None);
    assert_eq!(config.refresh_interval_secs, 30);
    assert!(config.notifications_enabled);
    assert!(!config.start_minimized);
    assert_eq!(config.theme, "default");

    let from_empty_json: Config = serde_json::from_str("{}").unwrap();
    assert_eq!(from_empty_json, config);
}

#[test]
fn path_resolution_prefers_explicit_override() {
    let override_path = Path::new("/tmp/custom-airgradient.json");
    let resolved = resolve_config_path_from(
        Some(override_path),
        Some(Path::new("/tmp/xdg")),
        Some(Path::new("/tmp/home")),
    )
    .unwrap();

    assert_eq!(resolved, override_path);
}

#[test]
fn path_resolution_uses_xdg_config_home() {
    let resolved = resolve_config_path_from(
        None,
        Some(Path::new("/tmp/xdg")),
        Some(Path::new("/tmp/home")),
    )
    .unwrap();

    assert_eq!(
        resolved,
        PathBuf::from("/tmp/xdg/airgradient-desktop/config.json")
    );
}

#[test]
fn path_resolution_falls_back_to_home_config() {
    let resolved = resolve_config_path_from(None, None, Some(Path::new("/tmp/home"))).unwrap();

    assert_eq!(
        resolved,
        PathBuf::from("/tmp/home/.config/airgradient-desktop/config.json")
    );
}

#[test]
fn refresh_interval_bounds_are_enforced() {
    assert!(validate_refresh_interval(MIN_REFRESH_INTERVAL_SECS).is_ok());
    assert!(validate_refresh_interval(DEFAULT_REFRESH_INTERVAL_SECS).is_ok());
    assert!(validate_refresh_interval(MAX_REFRESH_INTERVAL_SECS).is_ok());

    assert!(matches!(
        validate_refresh_interval(MIN_REFRESH_INTERVAL_SECS - 1),
        Err(ConfigError::RefreshIntervalOutOfRange(4))
    ));
    assert!(matches!(
        validate_refresh_interval(MAX_REFRESH_INTERVAL_SECS + 1),
        Err(ConfigError::RefreshIntervalOutOfRange(3601))
    ));
}

#[test]
fn read_config_rejects_out_of_bounds_refresh_interval() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("config.json");
    fs::write(
        &path,
        r#"{"server_url":null,"refresh_interval_secs":1,"notifications_enabled":true,"start_minimized":false}"#,
    )
    .unwrap();

    assert!(matches!(
        read_config(&path),
        Err(ConfigError::RefreshIntervalOutOfRange(1))
    ));
}

#[test]
fn read_missing_config_returns_defaults() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("missing.json");

    assert_eq!(read_config(&path).unwrap(), Config::default());
}

#[test]
fn read_write_json_round_trip() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("nested").join("config.json");
    let config = Config {
        server_url: Some("http://192.168.1.201".to_owned()),
        refresh_interval_secs: 60,
        notifications_enabled: false,
        start_minimized: true,
        theme: "nord".to_owned(),
    };

    write_config(&path, &config).unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&contents).unwrap();
    assert_eq!(json["server_url"], "http://192.168.1.201");
    assert_eq!(json["refresh_interval_secs"], 60);
    assert_eq!(json["notifications_enabled"], false);
    assert_eq!(json["start_minimized"], true);
    assert_eq!(json["theme"], "nord");
    assert_eq!(read_config(&path).unwrap(), config);
}

#[test]
fn set_url_normalizes_with_device_boundary_before_persisting() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("config.json");

    let config = set_url(&path, "192.168.1.201/sensors").unwrap();

    assert_eq!(config.server_url.as_deref(), Some("http://192.168.1.201/"));
    assert_eq!(
        read_config(&path).unwrap().server_url.as_deref(),
        Some("http://192.168.1.201/")
    );
}

#[test]
fn set_refresh_interval_persists_valid_value() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("config.json");

    let config = set_refresh_interval(&path, 120).unwrap();

    assert_eq!(config.refresh_interval_secs, 120);
    assert_eq!(read_config(&path).unwrap().refresh_interval_secs, 120);
}

#[test]
fn set_theme_persists_any_id_without_validation() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("config.json");

    let config = set_theme(&path, "dracula").unwrap();

    assert_eq!(config.theme, "dracula");
    assert_eq!(read_config(&path).unwrap().theme, "dracula");
}

#[test]
fn set_theme_preserves_other_fields() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("config.json");
    set_refresh_interval(&path, 90).unwrap();

    let config = set_theme(&path, "gruvbox").unwrap();

    assert_eq!(config.theme, "gruvbox");
    assert_eq!(config.refresh_interval_secs, 90);
}

#[test]
fn unknown_theme_value_type_warns_and_falls_back_to_default() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().join("config.json");
    fs::write(&path, r#"{"theme": 42}"#).unwrap();

    let display = read_display_config(&path).unwrap();

    assert_eq!(display.config.theme, "default");
    assert!(display.warnings.iter().any(|w| w.contains("theme")));
}
