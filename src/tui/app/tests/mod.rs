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
use crate::{config, sensors::Trend, tui::theme};

fn app_with_refresh(refresh_interval: Duration) -> TuiApp {
    TuiApp::new(None, refresh_interval)
}

fn app_with_url() -> TuiApp {
    TuiApp::new(
        Some(Url::parse("http://192.168.1.201/").expect("url should parse")),
        Duration::from_secs(30),
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
    let mut app = app_with_refresh(Duration::from_secs(30));
    app.config_path = Some(path.clone());
    (app, path)
}
