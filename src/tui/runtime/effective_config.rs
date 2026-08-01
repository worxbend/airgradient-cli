//! Resolves what the TUI should actually use at startup, merging the config
//! file with CLI overrides and falling back to defaults.
//!
//! Nothing here fails the launch: a broken config file or an unparseable URL
//! becomes an on-screen error while the TUI still starts, so the user can fix
//! it from the config editor instead of being dropped back to a shell.

use std::time::Duration;

use url::Url;

use crate::{
    config, device,
    tui::{runtime::RuntimeOptions, theme::Theme},
};

/// Lower bound for the test-only scheduler override. Anything faster would
/// make the loop spin hot enough to change the timing behavior under test.
const MIN_TUI_TEST_REFRESH_INTERVAL_MS: u64 = 100;
const MIN_TUI_TEST_REFRESH_INTERVAL: Duration =
    Duration::from_millis(MIN_TUI_TEST_REFRESH_INTERVAL_MS);

#[doc(hidden)]
/// Diagnostic/test-only scheduler override. Values below 100ms are ignored.
pub const TUI_TEST_REFRESH_INTERVAL_MS_ENV: &str = "AIRGRADIENT_CLI_TUI_TEST_REFRESH_INTERVAL_MS";

#[derive(Debug)]
pub(super) struct EffectiveConfig {
    pub(super) configured_url: Option<Url>,
    pub(super) refresh_interval: Duration,
    pub(super) theme: Theme,
    /// Whatever went wrong while resolving, surfaced in the UI's error panel.
    pub(super) current_error: Option<String>,
}

impl EffectiveConfig {
    pub(super) fn resolve(options: &RuntimeOptions) -> Self {
        let mut current_error = None;
        let mut configured_url = None;

        let config = match config::read_display_config(&options.config_path) {
            Ok(display) => {
                if !display.warnings.is_empty() {
                    current_error = Some(display.warnings.join("; "));
                }
                display.config
            }
            Err(error) => {
                current_error = Some(error.to_string());
                config::Config::default()
            }
        };

        let server_url = options
            .url_override
            .as_deref()
            .map(str::to_owned)
            .or(config.server_url);

        if let Some(server_url) = server_url.as_deref().filter(|url| !url.trim().is_empty()) {
            match device::normalize_base_url(server_url) {
                Ok(url) => configured_url = Some(url),
                Err(error) => current_error = Some(error.to_string()),
            }
        }

        let refresh_secs = options
            .refresh_override_secs
            .unwrap_or(config.refresh_interval_secs);

        let theme_id = options.theme_override.as_deref().unwrap_or(&config.theme);

        Self {
            configured_url,
            refresh_interval: Duration::from_secs(refresh_secs),
            theme: Theme::by_id(theme_id),
            current_error,
        }
    }
}

/// Interval the scheduler polls on, which is the production interval unless
/// the diagnostic env override asks for something strictly shorter.
///
/// The override is scheduler-only and deliberately one-directional: it can
/// speed a test up but never slow it down or lengthen the interval the app
/// model reports, so `TuiApp::refresh_interval` stays the production value.
/// Unparseable, zero, too-fast, equal, and longer values are all ignored.
pub(super) fn runtime_refresh_schedule_interval(
    production_interval: Duration,
    override_value: Option<&str>,
) -> Duration {
    let Some(millis) = override_value.and_then(|value| value.parse::<u64>().ok()) else {
        return production_interval;
    };

    let override_interval = Duration::from_millis(millis);
    let accepted_override = override_interval >= MIN_TUI_TEST_REFRESH_INTERVAL
        && override_interval < production_interval;

    if accepted_override {
        override_interval
    } else {
        production_interval
    }
}
