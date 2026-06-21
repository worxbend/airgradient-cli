use std::{fs, path::Path, time::Duration};

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::tempdir;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn cli() -> Command {
    Command::cargo_bin("airgradient-cli").expect("binary should build")
}

#[test]
fn fetch_requests_current_measures_endpoint() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime should start");
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/measures/current"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rco2": 612,
                "pm02": 7.4
            })))
            .expect(1)
            .mount(&server)
            .await;
        server
    });

    cli()
        .args(["--url", &server.uri(), "--no-color"])
        .assert()
        .success()
        .stdout(predicate::str::contains("AQI"));

    runtime.block_on(async {
        server.verify().await;
    });
}

#[test]
fn captured_text_output_disables_color_by_default() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime should start");
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/measures/current"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rco2": 612,
                "pm02": 7.4
            })))
            .mount(&server)
            .await;
        server
    });

    let assert = cli().args(["--url", &server.uri()]).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf8");

    assert!(stdout.contains("AQI"));
    assert!(stdout.contains("PM2.5"));
    assert!(!stdout.contains("\u{1b}["));
}

#[test]
fn fetch_json_emits_normalized_json() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime should start");
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/measures/current"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rco2": 612,
                "pm02": 7.4,
                "rhumCompensated": 48.1
            })))
            .mount(&server)
            .await;
        server
    });

    let assert = cli()
        .args(["--url", &server.uri(), "fetch", "--json"])
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf8");
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be JSON");

    assert_eq!(json["device_url"], format!("{}/", server.uri()));
    assert_eq!(json["snapshot"]["co2"], 612.0);
    assert_eq!(json["snapshot"]["pm25"], 7.4);
    assert_eq!(json["snapshot"]["pm1"], Value::Null);
    assert_eq!(json["metrics"][0]["key"], "aqi");
}

#[test]
fn missing_configured_url_exits_non_zero_with_setup_guidance() {
    let dir = tempdir().expect("tempdir should be created");
    let config_path = dir.path().join("missing.json");

    cli()
        .args(["--config", path_str(&config_path), "fetch"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("config set-url").and(predicate::str::contains("--url")));
}

#[test]
fn invalid_url_error_is_concise_by_default_and_verbose_shows_source_chain() {
    cli()
        .args(["--url", "http://[::1", "fetch"])
        .assert()
        .failure()
        .stdout("")
        .stderr(
            predicate::str::contains("error: invalid device URL")
                .and(predicate::str::contains("\u{1b}[").not())
                .and(predicate::str::contains("caused by:").not()),
        );

    cli()
        .args(["-v", "--url", "http://[::1", "fetch"])
        .assert()
        .failure()
        .stdout("")
        .stderr(
            predicate::str::contains("error: invalid device URL")
                .and(predicate::str::contains("caused by:"))
                .and(predicate::str::contains("\u{1b}[").not()),
        );
}

#[test]
fn unsupported_scheme_error_is_concise() {
    cli()
        .args(["--url", "ftp://192.168.1.201", "fetch"])
        .assert()
        .failure()
        .stdout("")
        .stderr(
            predicate::str::contains("unsupported device URL scheme `ftp`")
                .and(predicate::str::contains("\u{1b}[").not()),
        );
}

#[test]
fn non_success_http_status_exits_non_zero() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime should start");
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/measures/current"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        server
    });

    cli()
        .args(["--url", &server.uri(), "fetch"])
        .assert()
        .failure()
        .stdout("")
        .stderr(
            predicate::str::contains("non-success status 503")
                .and(predicate::str::contains("\u{1b}[").not()),
        );
}

#[test]
fn invalid_json_response_exits_non_zero() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime should start");
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/measures/current"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        server
    });

    cli()
        .args(["--url", &server.uri(), "fetch"])
        .assert()
        .failure()
        .stdout("")
        .stderr(
            predicate::str::contains("failed to parse AirGradient measurements JSON")
                .and(predicate::str::contains("\u{1b}[").not()),
        );
}

#[test]
fn timeout_error_exits_non_zero_without_slowing_suite() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime should start");
    let server = runtime.block_on(async {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/measures/current"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(100))
                    .set_body_json(serde_json::json!({ "pm02": 7.4 })),
            )
            .mount(&server)
            .await;
        server
    });

    cli()
        .env("AIRGRADIENT_CLI_FETCH_TIMEOUT_MS", "10")
        .args(["--url", &server.uri(), "fetch"])
        .assert()
        .failure()
        .stdout("")
        .stderr(
            predicate::str::contains("failed to request AirGradient measurements")
                .and(predicate::str::contains("\u{1b}[").not()),
        );
}

#[test]
fn refresh_without_tui_is_rejected_for_one_shot_fetch() {
    cli()
        .args(["--url", "192.168.1.201", "--refresh", "30", "fetch"])
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains(
            "`--refresh` is only supported with `--tui`",
        ));
}

#[test]
fn refresh_without_tui_is_rejected_for_config_commands() {
    cli()
        .args(["--refresh", "30", "config", "path"])
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains(
            "`--refresh` is only supported with `--tui`",
        ));
}

#[test]
fn top_level_json_is_rejected_for_config_commands() {
    let dir = tempdir().expect("tempdir should be created");
    let config_path = dir.path().join("config.json");

    cli()
        .args([
            "--config",
            path_str(&config_path),
            "--json",
            "config",
            "show",
        ])
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains(
            "`--json` only applies to one-shot fetch output",
        ));
}

#[test]
fn tui_is_accepted_but_reports_pending_implementation() {
    cli()
        .args(["--tui", "--refresh", "30"])
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("TUI is not implemented yet."));
}

#[test]
fn config_path_respects_explicit_alternate_path() {
    let dir = tempdir().expect("tempdir should be created");
    let config_path = dir.path().join("custom.json");

    cli()
        .args(["--config", path_str(&config_path), "config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains(path_str(&config_path)));
}

#[test]
fn config_path_respects_xdg_config_home() {
    let dir = tempdir().expect("tempdir should be created");
    let expected = dir.path().join("airgradient-desktop").join("config.json");

    cli()
        .env("XDG_CONFIG_HOME", dir.path())
        .env_remove("HOME")
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains(path_str(&expected)));
}

#[test]
fn config_set_url_writes_desktop_compatible_json() {
    let dir = tempdir().expect("tempdir should be created");
    let config_path = dir.path().join("config.json");

    cli()
        .args([
            "--config",
            path_str(&config_path),
            "config",
            "set-url",
            "192.168.1.201/sensors?debug=true#now",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(config_path).expect("config should be written");
    let json: Value = serde_json::from_str(&contents).expect("config should be JSON");

    assert_eq!(json["server_url"], "http://192.168.1.201/");
    assert_eq!(json["refresh_interval_secs"], 30);
    assert_eq!(json["notifications_enabled"], true);
    assert_eq!(json["start_minimized"], false);
}

#[test]
fn config_set_refresh_preserves_unknown_top_level_fields() {
    let dir = tempdir().expect("tempdir should be created");
    let config_path = dir.path().join("config.json");
    let original = serde_json::json!({
        "server_url": "http://192.168.1.201/",
        "refresh_interval_secs": 30,
        "notifications_enabled": false,
        "start_minimized": true,
        "future_desktop_field": "preserve someday"
    });
    fs::write(&config_path, original.to_string()).expect("config should be written");

    cli()
        .args([
            "--config",
            path_str(&config_path),
            "config",
            "set-refresh",
            "45",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(config_path).expect("config should be rewritten");
    let json: Value = serde_json::from_str(&contents).expect("config should be JSON");

    assert_eq!(json["server_url"], "http://192.168.1.201/");
    assert_eq!(json["refresh_interval_secs"], 45);
    assert_eq!(json["notifications_enabled"], false);
    assert_eq!(json["start_minimized"], true);
    assert_eq!(json["future_desktop_field"], "preserve someday");
}

#[test]
fn config_set_url_preserves_unknown_top_level_fields() {
    let dir = tempdir().expect("tempdir should be created");
    let config_path = dir.path().join("config.json");
    let original = serde_json::json!({
        "server_url": "http://192.168.1.201/",
        "refresh_interval_secs": 30,
        "notifications_enabled": false,
        "start_minimized": true,
        "future_desktop_field": {
            "nested": true
        }
    });
    fs::write(&config_path, original.to_string()).expect("config should be written");

    cli()
        .args([
            "--config",
            path_str(&config_path),
            "config",
            "set-url",
            "https://airgradient.local/sensors?debug=true#now",
        ])
        .assert()
        .success();

    let contents = fs::read_to_string(config_path).expect("config should be rewritten");
    let json: Value = serde_json::from_str(&contents).expect("config should be JSON");

    assert_eq!(json["server_url"], "https://airgradient.local/");
    assert_eq!(json["refresh_interval_secs"], 30);
    assert_eq!(json["notifications_enabled"], false);
    assert_eq!(json["start_minimized"], true);
    assert_eq!(json["future_desktop_field"]["nested"], true);
}

#[test]
fn config_show_prints_normalized_server_url_without_rewriting_file() {
    let dir = tempdir().expect("tempdir should be created");
    let config_path = dir.path().join("config.json");
    let original = serde_json::json!({
        "server_url": "http://192.168.1.201/foo?debug=true#now",
        "refresh_interval_secs": 30,
        "notifications_enabled": true,
        "start_minimized": false
    })
    .to_string();
    fs::write(&config_path, &original).expect("config should be written");

    cli()
        .args(["--config", path_str(&config_path), "config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("http://192.168.1.201/"));

    let contents = fs::read_to_string(config_path).expect("config should remain readable");
    assert_eq!(contents, original);
}

#[test]
fn config_show_prints_unsupported_scheme_with_warning() {
    let dir = tempdir().expect("tempdir should be created");
    let config_path = dir.path().join("config.json");
    let original = serde_json::json!({
        "server_url": "ftp://192.168.1.201",
        "refresh_interval_secs": 30,
        "notifications_enabled": true,
        "start_minimized": false
    })
    .to_string();
    fs::write(&config_path, &original).expect("config should be written");

    let assert = cli()
        .args(["--config", path_str(&config_path), "config", "show"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("warning:").and(predicate::str::contains(
                "unsupported device URL scheme `ftp`",
            )),
        );

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf8");
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(json["server_url"], "ftp://192.168.1.201");

    let contents = fs::read_to_string(config_path).expect("config should remain readable");
    assert_eq!(contents, original);
}

#[test]
fn config_show_prints_malformed_url_with_warning() {
    let dir = tempdir().expect("tempdir should be created");
    let config_path = dir.path().join("config.json");
    let original = serde_json::json!({
        "server_url": "http://[::1",
        "refresh_interval_secs": 30,
        "notifications_enabled": true,
        "start_minimized": false
    })
    .to_string();
    fs::write(&config_path, &original).expect("config should be written");

    let assert = cli()
        .args(["--config", path_str(&config_path), "config", "show"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("warning:")
                .and(predicate::str::contains("invalid device URL `http://[::1`")),
        );

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf8");
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(json["server_url"], "http://[::1");

    let contents = fs::read_to_string(config_path).expect("config should remain readable");
    assert_eq!(contents, original);
}

#[test]
fn config_show_prints_empty_server_url_without_warning() {
    let dir = tempdir().expect("tempdir should be created");
    let config_path = dir.path().join("config.json");
    fs::write(
        &config_path,
        serde_json::json!({
            "server_url": "",
            "refresh_interval_secs": 30,
            "notifications_enabled": false,
            "start_minimized": true
        })
        .to_string(),
    )
    .expect("config should be written");

    let assert = cli()
        .args(["--config", path_str(&config_path), "config", "show"])
        .assert()
        .success()
        .stderr("");

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf8");
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(json["server_url"], "");
    assert_eq!(json["notifications_enabled"], false);
    assert_eq!(json["start_minimized"], true);
}

#[test]
fn config_show_prints_missing_server_url_without_warning() {
    let dir = tempdir().expect("tempdir should be created");
    let config_path = dir.path().join("config.json");
    fs::write(
        &config_path,
        serde_json::json!({
            "refresh_interval_secs": 45,
            "notifications_enabled": false,
            "start_minimized": true
        })
        .to_string(),
    )
    .expect("config should be written");

    let assert = cli()
        .args(["--config", path_str(&config_path), "config", "show"])
        .assert()
        .success()
        .stderr("");

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("stdout is utf8");
    let json: Value = serde_json::from_str(&stdout).expect("stdout should be JSON");
    assert_eq!(json["server_url"], Value::Null);
    assert_eq!(json["refresh_interval_secs"], 45);
    assert_eq!(json["notifications_enabled"], false);
    assert_eq!(json["start_minimized"], true);
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("test path should be utf8")
}
