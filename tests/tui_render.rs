use std::time::{Duration, SystemTime};

use airgradient_cli::{
    device::FetchSettings,
    sensors::SensorSnapshot,
    tui::{app::TuiApp, ui},
};
use ratatui::{Terminal, backend::TestBackend};
use reqwest::Client;
use url::Url;

fn render(app: &TuiApp) -> String {
    let backend = TestBackend::new(100, 40);
    let mut terminal = Terminal::new(backend).expect("test terminal should be created");

    terminal
        .draw(|frame| ui::draw(frame, app))
        .expect("dashboard should render");

    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

fn app(configured_url: Option<Url>) -> TuiApp {
    TuiApp::with_client(
        configured_url,
        Duration::from_secs(30),
        FetchSettings::default(),
        Client::new(),
    )
}

fn populated_snapshot() -> SensorSnapshot {
    SensorSnapshot {
        aqi: Some(42.0),
        co2: Some(612.0),
        pm25: Some(7.4),
        pm1: Some(4.1),
        pm10: Some(9.8),
        pm03_count: Some(1234.0),
        tvoc: Some(82.0),
        nox: Some(3.0),
        temperature_c: Some(22.6),
        humidity: Some(48.1),
    }
}

#[test]
fn renders_useful_dashboard_without_configured_url() {
    let output = render(&app(None));

    assert!(output.contains("AirGradient"));
    assert!(output.contains("no device URL configured"));
    assert!(output.contains("missing config"));
    assert!(output.contains("--"));
    assert!(output.contains("r refresh"));
}

#[test]
fn renders_populated_snapshot() {
    let mut app = app(Some(
        Url::parse("http://192.168.1.201/").expect("url should parse"),
    ));
    app.apply_success(
        populated_snapshot(),
        Duration::from_millis(125),
        SystemTime::now(),
    );

    let output = render(&app);

    assert!(output.contains("AirGradient"));
    assert!(output.contains("http://192.168.1.201"));
    assert!(output.contains("AQI"));
    assert!(output.contains("42"));
    assert!(output.contains("CO2"));
    assert!(output.contains("612"));
    assert!(output.contains("PM2.5"));
    assert!(output.contains("7.4"));
}

#[test]
fn renders_active_error_without_dropping_previous_successful_snapshot() {
    let mut app = app(Some(
        Url::parse("http://192.168.1.201/").expect("url should parse"),
    ));
    app.apply_success(
        populated_snapshot(),
        Duration::from_millis(110),
        SystemTime::now(),
    );
    app.apply_failure("request timed out", Duration::from_millis(250));

    let output = render(&app);

    assert!(output.contains("fetch failed"));
    assert!(output.contains("Current Error"));
    assert!(output.contains("request timed out"));
    assert!(output.contains("AQI"));
    assert!(output.contains("42"));
    assert!(output.contains("CO2"));
    assert!(output.contains("612"));
}
