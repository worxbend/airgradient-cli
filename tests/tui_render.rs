use std::time::{Duration, SystemTime};

use airgradient_cli::{
    device::FetchSettings,
    sensors::SensorSnapshot,
    tui::{app::TuiApp, ui},
};
use ratatui::{Terminal, backend::TestBackend};
use reqwest::Client;
use url::Url;

struct Rendered {
    output: String,
    lines: Vec<String>,
}

fn render(app: &TuiApp) -> Rendered {
    render_at(app, 100, 40)
}

fn render_at(app: &TuiApp, width: u16, height: u16) -> Rendered {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should be created");

    terminal
        .draw(|frame| ui::draw(frame, app))
        .expect("dashboard should render");

    let buffer = terminal.backend().buffer();
    let mut output = String::new();
    let mut lines = Vec::new();

    for row in 0..height {
        let mut line = String::new();
        for column in 0..width {
            line.push_str(buffer[(column, row)].symbol());
        }
        output.push_str(&line);
        output.push('\n');
        lines.push(line);
    }

    Rendered { output, lines }
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

fn assert_nonblank(rendered: &Rendered) {
    assert!(
        rendered
            .output
            .chars()
            .any(|character| !character.is_whitespace()),
        "rendered buffer should not be blank"
    );
}

fn assert_any_line_contains(rendered: &Rendered, expected: &str) {
    assert!(
        rendered.lines.iter().any(|line| line.contains(expected)),
        "expected a rendered line to contain {expected:?}\n{}",
        rendered.output
    );
}

#[test]
fn renders_useful_dashboard_without_configured_url() {
    let output = render(&app(None));

    assert_nonblank(&output);
    assert!(output.output.contains("AirGradient"));
    assert!(output.output.contains("no device URL configured"));
    assert!(output.output.contains("missing config"));
    assert!(output.output.contains("--"));
    assert!(output.output.contains("r refresh"));
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

    assert_nonblank(&output);
    assert!(output.output.contains("AirGradient"));
    assert!(output.output.contains("http://192.168.1.201"));
    assert!(output.output.contains("AQI"));
    assert!(output.output.contains("42"));
    assert!(output.output.contains("CO2"));
    assert!(output.output.contains("612"));
    assert!(output.output.contains("PM2.5"));
    assert!(output.output.contains("7.4"));
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

    assert_nonblank(&output);
    assert!(output.output.contains("fetch failed"));
    assert!(output.output.contains("Current Error"));
    assert!(output.output.contains("request timed out"));
    assert!(output.output.contains("AQI"));
    assert!(output.output.contains("42"));
    assert!(output.output.contains("CO2"));
    assert!(output.output.contains("612"));
}

#[test]
fn renders_requested_compact_sizes_without_panics() {
    let mut app = app(Some(
        Url::parse("http://192.168.1.201/").expect("url should parse"),
    ));
    app.apply_success(
        populated_snapshot(),
        Duration::from_millis(125),
        SystemTime::now(),
    );

    for (width, height) in [(60, 20), (80, 24), (36, 20)] {
        let output = render_at(&app, width, height);

        assert_nonblank(&output);
        assert_any_line_contains(&output, "AirGradient");
        assert!(output.output.contains("AQI"));
        assert!(output.output.contains("42"));
    }
}

#[test]
fn truncates_long_configured_url_predictably_on_compact_width() {
    let mut app = app(Some(
        Url::parse("https://very-long-airgradient-device-name.example.internal:8443/")
            .expect("url should parse"),
    ));
    app.apply_success(
        populated_snapshot(),
        Duration::from_millis(125),
        SystemTime::now(),
    );

    let output = render_at(&app, 60, 20);

    assert_nonblank(&output);
    assert_any_line_contains(&output, "AirGradient");
    assert_any_line_contains(&output, "https://very-long-airgradient-device");
    assert!(
        !output.output.contains("example.internal:8443"),
        "long URL tail should be clipped at compact width\n{}",
        output.output
    );
}

#[test]
fn renders_all_missing_metric_values_after_success() {
    let mut app = app(Some(
        Url::parse("http://192.168.1.201/").expect("url should parse"),
    ));
    app.apply_success(
        SensorSnapshot::default(),
        Duration::from_millis(125),
        SystemTime::now(),
    );

    let output = render(&app);

    assert_nonblank(&output);
    assert!(output.output.contains("AQI"));
    assert!(output.output.contains("--"));
    assert!(output.output.contains("Unknown"));
    assert!(output.output.contains("CO2"));
    assert!(output.output.contains("PM2.5"));
    assert!(output.output.contains("Humidity"));
}

#[test]
fn renders_long_config_error_without_losing_controls() {
    let mut app = app(None);
    app.apply_failure(
        "configuration error: server_url contains an unsupported scheme and a very long explanatory message",
        Duration::from_millis(5),
    );

    let output = render_at(&app, 60, 20);

    assert_nonblank(&output);
    assert!(output.output.contains("no device URL configured"));
    assert!(
        !output.output.contains("missing config"),
        "compact top bar should clip the trailing missing-config status\n{}",
        output.output
    );
    assert!(output.output.contains("Current Error"));
    assert!(output.output.contains("configuration error"));
    assert!(output.output.contains("r refresh"));
    assert!(output.output.contains("q quit"));
}

#[test]
fn renders_long_fetch_error_with_last_success_on_narrow_width() {
    let mut app = app(Some(
        Url::parse("http://192.168.1.201/").expect("url should parse"),
    ));
    app.apply_success(
        populated_snapshot(),
        Duration::from_millis(110),
        SystemTime::now(),
    );
    app.apply_failure(
        "request failed after retries: connection timed out while reading response headers from the device",
        Duration::from_millis(250),
    );

    let output = render_at(&app, 36, 20);

    assert_nonblank(&output);
    assert!(
        !output.output.contains("fetch failed"),
        "narrow top bar should clip the trailing fetch-failed status\n{}",
        output.output
    );
    assert!(output.output.contains("Current Error"));
    assert!(output.output.contains("request failed"));
    assert!(output.output.contains("AQI"));
    assert!(output.output.contains("42"));
}
