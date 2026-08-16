//! Tests for the app model, split by the state machine each one exercises.
//!
//! The shared constructors live here so every child module builds its `TuiApp`
//! the same way.

mod config_editor;
mod fetch;
mod leader;
mod palette;
mod theme_settings;

use super::*;
use crate::{
    config,
    device_url::DeviceUrl,
    refresh_interval::{MAX_SECS as MAX_REFRESH_INTERVAL_SECS, RefreshInterval},
    sensors::Trend,
    tui::theme,
};

fn app_with_refresh(refresh_interval: RefreshInterval) -> TuiApp {
    TuiApp::new(None, refresh_interval)
}

/// A refresh interval for tests, clamped rather than rejected so a case can
/// name an out-of-range number and still get a value to hand to the model.
fn refresh(seconds: u64) -> RefreshInterval {
    RefreshInterval::clamping(Duration::from_secs(seconds))
}

fn app_with_url() -> TuiApp {
    TuiApp::new(
        Some(DeviceUrl::parse("http://192.168.1.201/").expect("url should parse")),
        refresh(30),
    )
}

fn snapshot(aqi: f64, co2: f64) -> SensorSnapshot {
    SensorSnapshot {
        aqi: Some(aqi),
        co2: Some(co2),
        ..SensorSnapshot::default()
    }
}

fn app_with_config(dir: &std::path::Path) -> (TuiApp, std::path::PathBuf) {
    let path = dir.join("config.json");
    let mut app = app_with_refresh(refresh(30));
    app.config_path = Some(path.clone());
    (app, path)
}
