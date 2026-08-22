//! Resolves what the TUI should actually use at startup, merging the config
//! file with CLI overrides and falling back to defaults.
//!
//! Nothing here fails the launch: a broken config file or an unparseable URL
//! becomes an on-screen error while the TUI still starts, so the user can fix
//! it from the config editor instead of being dropped back to a shell.

use std::time::Duration;

use crate::{
    config,
    device_url::DeviceUrl,
    refresh_interval::RefreshInterval,
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
    pub(super) configured_url: Option<DeviceUrl>,
    pub(super) refresh_interval: RefreshInterval,
    pub(super) theme: Theme,
    /// Whatever went wrong while resolving, surfaced in the UI's error panel.
    pub(super) current_error: Option<String>,
}

impl EffectiveConfig {
    pub(super) fn resolve(options: &RuntimeOptions) -> Self {
        // Collected rather than assigned: startup can turn up more than one
        // problem, and a single slot meant each one silently replaced the
        // last, so a bad `--url` hid a broken config file behind it.
        let mut problems: Vec<String> = Vec::new();
        let mut configured_url = None;

        let config = match config::read_display_config(&options.config_path) {
            Ok(display) => {
                problems.extend(display.warnings);
                display.config
            }
            Err(error) => {
                problems.push(error.to_string());
                config::Config::default()
            }
        };

        let server_url = options
            .url_override
            .as_deref()
            .map(str::to_owned)
            .or(config.server_url);

        // The free function rather than `Config::device_url`, because the rule
        // applies to the override merged over the stored value, not to the
        // stored value alone.
        match DeviceUrl::parse_optional(server_url.as_deref()) {
            Ok(url) => configured_url = url,
            // Only when the bad URL came from the command line. A bad *stored*
            // URL has already been reported by the config layer above, which
            // hands back the original so the user can see what needs fixing --
            // reporting it here as well would say the same thing twice.
            Err(error) => {
                if options.url_override.is_some() {
                    problems.push(error.to_string());
                }
            }
        }

        // A `--refresh` override out of range is pulled into range rather than
        // failing the launch, like every other resolution problem here: the
        // point of this module is that the TUI always starts.
        let refresh_interval = match options.refresh_override_secs {
            Some(seconds) => match RefreshInterval::from_secs(seconds) {
                Ok(interval) => interval,
                Err(error) => {
                    problems.push(error.to_string());
                    RefreshInterval::clamping(Duration::from_secs(seconds))
                }
            },
            None => config.refresh_interval,
        };

        let theme_id = options.theme_override.as_deref().unwrap_or(&config.theme);

        Self {
            configured_url,
            refresh_interval,
            theme: Theme::by_id(theme_id),
            current_error: (!problems.is_empty()).then(|| problems.join("; ")),
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::device::FetchSettings;

    fn options(config_path: PathBuf, url_override: Option<&str>) -> RuntimeOptions {
        RuntimeOptions {
            config_path,
            history_path: None,
            record_path: None,
            url_override: url_override.map(str::to_owned),
            refresh_override_secs: None,
            theme_override: None,
            fetch_settings: FetchSettings::default(),
        }
    }

    fn config_file(dir: &std::path::Path, contents: &str) -> PathBuf {
        let path = dir.join("config.json");
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn every_startup_problem_is_reported_not_just_the_last_one() {
        // A single `Option` accumulator meant the URL error, resolved last,
        // silently replaced the config warning found before it.
        let temp_dir = tempfile::tempdir().unwrap();
        let path = config_file(temp_dir.path(), r#"{"theme": 7}"#);

        let resolved = EffectiveConfig::resolve(&options(path, Some("not a url")));

        let error = resolved.current_error.expect("both problems should show");
        assert!(error.contains("theme"), "{error}");
        assert!(
            error.contains("not a url") || error.to_lowercase().contains("url"),
            "{error}"
        );
    }

    #[test]
    fn a_bad_stored_url_is_reported_once_not_twice() {
        // The config layer already warns about an unparseable stored URL and
        // hands the original back, so the URL parse here must stay quiet.
        let temp_dir = tempfile::tempdir().unwrap();
        let path = config_file(temp_dir.path(), r#"{"server_url": "not a url"}"#);

        let resolved = EffectiveConfig::resolve(&options(path, None));

        let error = resolved.current_error.expect("the stored URL is bad");
        assert!(!error.contains("; "), "reported twice: {error}");
    }
}
