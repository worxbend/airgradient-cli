//! One fetch, then one of the one-shot renderers.
//!
//! Waybar is the odd exit: a failed fetch still prints JSON and returns `Ok`,
//! because Waybar clears the module when the process exits non-zero. The cause
//! goes to stderr. Text and JSON keep the error as `Err` so the process exits 1.

use std::io::{self, IsTerminal};
use std::time::{Instant, SystemTime};

use crate::device_url::DeviceUrl;
use crate::output::OutputMetadata;
use crate::sensors;

use super::{
    Cli, CliError, OneShotFormat, config_path, effective_config, history_path, record_reading,
    required_server_url,
};

struct FetchedReading {
    snapshot: sensors::SensorSnapshot,
    previous: Option<sensors::SensorSnapshot>,
    device_url: DeviceUrl,
    fetch_duration: std::time::Duration,
}

pub(super) async fn run_fetch(
    cli: &Cli,
    format: OneShotFormat,
    fetch_settings: crate::device::FetchSettings,
) -> Result<(), CliError> {
    let waybar_format = super::waybar_format_template(cli)?;
    match fetch_reading(cli, fetch_settings).await {
        Ok(fetched) => render_fetched(cli, format, &fetched, waybar_format),
        Err(error) if format == OneShotFormat::Waybar => waybar_failure(&error, waybar_format),
        Err(error) => Err(error),
    }
}

async fn fetch_reading(
    cli: &Cli,
    fetch_settings: crate::device::FetchSettings,
) -> Result<FetchedReading, CliError> {
    let path = config_path(cli)?;
    let config = effective_config(&path, cli)?;
    let device_url = required_server_url(&config)?;
    let client = fetch_settings.client()?;

    let started = Instant::now();
    let payload = crate::device::fetch_current_measures_with_client(&client, &device_url).await?;
    let fetch_duration = started.elapsed();
    let snapshot = sensors::parse_snapshot(&payload);
    let previous = record_reading(&snapshot).map(|reading| reading.snapshot);

    Ok(FetchedReading {
        snapshot,
        previous,
        device_url,
        fetch_duration,
    })
}

fn render_fetched(
    cli: &Cli,
    format: OneShotFormat,
    fetched: &FetchedReading,
    waybar_format: Option<&str>,
) -> Result<(), CliError> {
    let metadata = OutputMetadata {
        device_url: Some(fetched.device_url.as_str()),
        last_update: None,
        fetch_duration: Some(fetched.fetch_duration),
    };
    let previous = fetched.previous.as_ref();

    match format {
        OneShotFormat::Json => {
            println!(
                "{}",
                crate::output::json::render_pretty(&fetched.snapshot, previous, metadata)?
            );
        }
        OneShotFormat::Text => {
            let no_color = cli.no_color || !io::stdout().is_terminal();
            print!(
                "{}",
                crate::output::text::render(&fetched.snapshot, previous, metadata, no_color)
            );
        }
        OneShotFormat::Waybar => {
            println!(
                "{}",
                crate::output::waybar::render_live_formatted(
                    &fetched.snapshot,
                    metadata,
                    waybar_format,
                )?
            );
        }
    }

    Ok(())
}

pub(super) fn waybar_failure(
    error: &CliError,
    waybar_format: Option<&str>,
) -> Result<(), CliError> {
    eprintln!("error: {error}");
    let json = if shows_cached_reading(error) {
        match latest_reading() {
            Some(reading) => {
                let age =
                    crate::output::waybar::format_reading_age(reading.at(), SystemTime::now());
                crate::output::waybar::render_stale_formatted(
                    &reading.snapshot,
                    OutputMetadata::default(),
                    &error.to_string(),
                    &age,
                    waybar_format,
                )?
            }
            None => crate::output::waybar::render_offline(&error.to_string()),
        }
    } else {
        crate::output::waybar::render_offline(&error.to_string())
    };
    println!("{json}");
    Ok(())
}

/// A missing URL is "not configured", not "the device missed a poll".
/// An old reading must not keep looking like the bar's current device.
fn shows_cached_reading(error: &CliError) -> bool {
    !matches!(error, CliError::MissingServerUrl)
}

fn latest_reading() -> Option<crate::history::Reading> {
    let path = history_path()?;
    crate::history::load_recent(&path, 1).ok()?.pop()
}
