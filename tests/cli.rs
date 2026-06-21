use std::{fs, path::Path};

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
fn mutating_config_command_rewrites_known_desktop_shape_only() {
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
    let object = json.as_object().expect("config should be a JSON object");

    assert_eq!(object.len(), 4);
    assert_eq!(json["server_url"], "http://192.168.1.201/");
    assert_eq!(json["refresh_interval_secs"], 45);
    assert_eq!(json["notifications_enabled"], false);
    assert_eq!(json["start_minimized"], true);
    assert!(!object.contains_key("future_desktop_field"));
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

fn path_str(path: &Path) -> &str {
    path.to_str().expect("test path should be utf8")
}
