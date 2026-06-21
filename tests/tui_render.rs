use std::time::{Duration, SystemTime};

use airgradient_cli::{
    sensors::SensorSnapshot,
    tui::{app::TuiApp, theme, ui},
};
use ratatui::{Terminal, backend::TestBackend};
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
    TuiApp::new(configured_url, Duration::from_secs(30))
}

fn device_url() -> Url {
    Url::parse("http://192.168.1.201/").expect("url should parse")
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

fn region_text(rendered: &Rendered, x: usize, y: usize, width: usize, height: usize) -> String {
    let mut text = String::new();
    for line in rendered.lines.iter().skip(y).take(height) {
        let segment = line.chars().skip(x).take(width).collect::<String>();
        text.push_str(&segment);
        text.push('\n');
    }
    text
}

fn assert_region_contains(
    rendered: &Rendered,
    name: &str,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    expected: &str,
) {
    let text = region_text(rendered, x, y, width, height);
    assert!(
        text.contains(expected),
        "expected {name} region to contain {expected:?}\nregion:\n{text}\nfull render:\n{}",
        rendered.output
    );
}

fn assert_region_excludes(
    rendered: &Rendered,
    name: &str,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    unexpected: &str,
) {
    let text = region_text(rendered, x, y, width, height);
    assert!(
        !text.contains(unexpected),
        "expected {name} region to exclude {unexpected:?}\nregion:\n{text}\nfull render:\n{}",
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
fn renders_missing_config_without_pending_fetch_status() {
    let mut app = app(None);
    app.begin_fetch();

    let output = render(&app);

    assert_nonblank(&output);
    assert!(output.output.contains("missing config"));
    assert!(!output.output.contains("fetching"));
    assert!(!output.output.contains("refreshing"));
    assert!(output.output.contains("Set a device URL"));
}

#[test]
fn renders_initial_fetching_status_without_snapshot() {
    let mut app = app(Some(device_url()));
    app.begin_fetch();

    let output = render(&app);

    assert_nonblank(&output);
    assert!(output.output.contains("fetching"));
    assert!(
        output
            .output
            .contains("Fetching the first air quality reading.")
    );
    assert!(output.output.contains("--"));
    assert!(!output.output.contains("fetch failed"));
}

#[test]
fn renders_refreshing_status_while_preserving_previous_success() {
    let mut app = app(Some(device_url()));
    app.finish_fetch_success(
        populated_snapshot(),
        Duration::from_millis(125),
        SystemTime::now(),
    );
    app.begin_fetch();

    let output = render(&app);

    assert_nonblank(&output);
    assert!(output.output.contains("refreshing"));
    assert!(
        output
            .output
            .contains("Refreshing; showing the latest successful reading.")
    );
    assert!(output.output.contains("AQI"));
    assert!(output.output.contains("42"));
    assert!(output.output.contains("CO2"));
    assert!(output.output.contains("612"));
    assert!(!output.output.contains("waiting for first fetch"));
}

#[test]
fn renders_populated_snapshot() {
    let mut app = app(Some(device_url()));
    app.finish_fetch_success(
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
    let mut app = app(Some(device_url()));
    app.finish_fetch_success(
        populated_snapshot(),
        Duration::from_millis(110),
        SystemTime::now(),
    );
    app.finish_fetch_failure("request timed out", Duration::from_millis(250));

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
fn renders_retry_after_initial_failure_without_stale_completed_failure_status() {
    let mut app = app(Some(device_url()));
    app.finish_fetch_failure("request timed out", Duration::from_millis(250));
    app.begin_fetch();

    let output = render(&app);

    assert_nonblank(&output);
    assert!(output.output.contains("retrying"));
    assert!(output.output.contains("Retrying After Error"));
    assert!(output.output.contains("Retrying now; previous error:"));
    assert!(output.output.contains("request timed out"));
    assert!(
        output
            .output
            .contains("Retrying after a fetch failure; waiting for first successful reading.")
    );
    assert!(output.output.contains("--"));
    assert!(!output.output.contains("fetch failed"));
    assert!(!output.output.contains("Current Error"));
    assert!(
        !output
            .output
            .contains("Fetching the first air quality reading.")
    );
}

#[test]
fn renders_retry_after_failure_with_retained_last_success() {
    let mut app = app(Some(device_url()));
    app.finish_fetch_success(
        populated_snapshot(),
        Duration::from_millis(110),
        SystemTime::now(),
    );
    app.finish_fetch_failure("request timed out", Duration::from_millis(250));
    app.begin_fetch();

    let output = render(&app);

    assert_nonblank(&output);
    assert!(output.output.contains("retrying"));
    assert!(output.output.contains("Retrying After Error"));
    assert!(output.output.contains("request timed out"));
    assert!(
        output
            .output
            .contains("Retrying after a fetch failure; showing the latest successful reading.")
    );
    assert!(output.output.contains("AQI"));
    assert!(output.output.contains("42"));
    assert!(output.output.contains("CO2"));
    assert!(output.output.contains("612"));
    assert!(!output.output.contains("fetch failed"));
    assert!(
        !output
            .output
            .contains("Refreshing; showing the latest successful reading.")
    );
}

#[test]
fn renders_latest_failure_after_retry_as_completed_failure() {
    let mut app = app(Some(device_url()));
    app.finish_fetch_success(
        populated_snapshot(),
        Duration::from_millis(110),
        SystemTime::now(),
    );
    app.finish_fetch_failure("request timed out", Duration::from_millis(250));
    app.begin_fetch();
    app.finish_fetch_failure("connection refused", Duration::from_millis(75));

    let output = render(&app);

    assert_nonblank(&output);
    assert!(output.output.contains("fetch failed"));
    assert!(output.output.contains("Current Error"));
    assert!(output.output.contains("connection refused"));
    assert!(!output.output.contains("Retrying After Error"));
    assert!(!output.output.contains("request timed out"));
    assert!(output.output.contains("AQI"));
    assert!(output.output.contains("42"));
}

#[test]
fn renders_requested_compact_sizes_without_panics() {
    let mut app = app(Some(device_url()));
    app.finish_fetch_success(
        populated_snapshot(),
        Duration::from_millis(125),
        SystemTime::now(),
    );

    for (width, height) in [(60, 20), (80, 24), (36, 20)] {
        let output = render_at(&app, width, height);
        let width = usize::from(width);
        let height = usize::from(height);

        assert_nonblank(&output);
        assert_region_contains(&output, "top bar", 0, 0, width, 3, "AirGradient");
        if width >= 60 {
            assert_region_contains(&output, "top bar", 0, 0, width, 3, "refresh");
        }

        assert_region_contains(&output, "AQI panel", 0, 3, width, 7, "Air Quality");
        assert_region_contains(&output, "AQI panel", 0, 3, width, 7, "AQI");
        assert_region_contains(&output, "AQI panel", 0, 3, width, 7, "42");
        assert_region_contains(
            &output,
            "AQI panel",
            0,
            3,
            width,
            7,
            "Latest air quality reading.",
        );

        assert_region_excludes(&output, "footer", 0, height - 3, width, 3, "CO2");

        assert_region_contains(&output, "footer", 0, height - 3, width, 3, "r refresh");
        assert_region_contains(&output, "footer", 0, height - 3, width, 3, "q quit");
        assert_region_excludes(&output, "footer", 0, height - 3, width, 3, "AQI");
    }
}

#[test]
fn renders_dashboard_regions_at_compact_supported_size() {
    let mut app = app(Some(device_url()));
    app.finish_fetch_success(
        populated_snapshot(),
        Duration::from_millis(125),
        SystemTime::now(),
    );

    let output = render_at(&app, 80, 24);

    assert_nonblank(&output);
    assert_region_contains(&output, "top bar", 0, 0, 80, 3, "AirGradient");
    assert_region_contains(&output, "top bar", 0, 0, 80, 3, "http://192.168.1.201");
    assert_region_contains(&output, "top bar", 0, 0, 80, 3, "refresh 30s");
    assert_region_contains(&output, "top bar", 0, 0, 80, 3, "updated");

    assert_region_contains(&output, "AQI panel", 0, 3, 80, 7, "Air Quality");
    assert_region_contains(&output, "AQI panel", 0, 3, 80, 7, "AQI 42");
    assert_region_contains(&output, "AQI panel", 0, 3, 80, 7, "Good");
    assert_region_contains(
        &output,
        "AQI panel",
        0,
        3,
        80,
        7,
        "Latest air quality reading.",
    );

    assert_region_contains(&output, "metric grid", 0, 10, 80, 11, "PM10");
    assert_region_contains(&output, "metric grid", 0, 10, 80, 11, "Good");
    assert_region_contains(&output, "metric grid", 0, 10, 80, 11, "PM0.3 count");
    assert_region_contains(&output, "metric grid", 0, 10, 80, 11, "Elevated");
    assert_region_excludes(&output, "metric grid", 0, 10, 80, 11, "r refresh");

    assert_region_contains(&output, "footer", 0, 21, 80, 3, "r refresh");
    assert_region_contains(&output, "footer", 0, 21, 80, 3, "q quit");
    assert_region_contains(&output, "footer", 0, 21, 80, 3, "Esc quit");
    assert_region_excludes(&output, "footer", 0, 21, 80, 3, "CO2");
}

#[test]
fn renders_below_minimum_terminal_fallback_without_dashboard_overlap() {
    let mut app = app(Some(device_url()));
    app.finish_fetch_success(
        populated_snapshot(),
        Duration::from_millis(125),
        SystemTime::now(),
    );

    for (width, height) in [
        (theme::MIN_TERMINAL_WIDTH - 1, theme::MIN_TERMINAL_HEIGHT),
        (theme::MIN_TERMINAL_WIDTH, theme::MIN_TERMINAL_HEIGHT - 1),
        (24, 8),
    ] {
        let output = render_at(&app, width, height);

        assert_nonblank(&output);
        assert_any_line_contains(&output, "Terminal too small");
        assert_any_line_contains(
            &output,
            &format!(
                "Minimum {}x{}",
                theme::MIN_TERMINAL_WIDTH,
                theme::MIN_TERMINAL_HEIGHT
            ),
        );
        assert!(!output.output.contains("Air Quality"));
        assert!(!output.output.contains("r refresh"));
        assert!(!output.output.contains("http://192.168.1.201"));
    }
}

#[test]
fn renders_error_panel_in_dedicated_region_at_compact_size() {
    let mut app = app(Some(device_url()));
    app.finish_fetch_success(
        populated_snapshot(),
        Duration::from_millis(110),
        SystemTime::now(),
    );
    app.finish_fetch_failure("request timed out", Duration::from_millis(250));

    let output = render_at(&app, 60, 20);

    assert_nonblank(&output);
    assert_region_contains(&output, "top bar", 0, 0, 60, 3, "AirGradient");
    assert_region_contains(&output, "top bar", 0, 0, 60, 3, "refresh 30s");
    assert_region_contains(&output, "AQI panel", 0, 3, 60, 7, "AQI");
    assert_region_contains(&output, "error panel", 0, 12, 60, 5, "Current Error");
    assert_region_contains(&output, "error panel", 0, 12, 60, 5, "request timed out");
    assert_region_excludes(&output, "AQI panel", 0, 3, 60, 7, "request timed out");
    assert_region_contains(&output, "footer", 0, 17, 60, 3, "r refresh");
    assert_region_contains(&output, "footer", 0, 17, 60, 3, "Esc quit");
}

#[test]
fn truncates_long_configured_url_predictably_on_compact_width() {
    let mut app = app(Some(
        Url::parse("https://very-long-airgradient-device-name.example.internal:8443/")
            .expect("url should parse"),
    ));
    app.finish_fetch_success(
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
    let mut app = app(Some(device_url()));
    app.finish_fetch_success(
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
    app.finish_fetch_failure(
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
    let mut app = app(Some(device_url()));
    app.finish_fetch_success(
        populated_snapshot(),
        Duration::from_millis(110),
        SystemTime::now(),
    );
    app.finish_fetch_failure(
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
