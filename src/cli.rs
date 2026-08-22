//! Argument parsing and command dispatch.
//!
//! This is where flag combinations are validated, before anything runs: the
//! options that only make sense for the dashboard, and the ones that only make
//! sense for one-shot output, are mutually exclusive and rejected here rather
//! than silently ignored later.

use std::{
    env,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

use clap::{Args, Parser, Subcommand};
use thiserror::Error;

use crate::{
    config::{self, Config},
    device,
    device_url::{DeviceUrl, DeviceUrlError},
    history,
    output::{self, OutputMetadata},
    sensors, tui,
};

/// Set to anything to stop a one-shot fetch recording what it read.
const NO_HISTORY_ENV: &str = "AIRGRADIENT_CLI_NO_HISTORY";

#[derive(Debug, Parser)]
#[command(version, about = "Fetch and render AirGradient sensor readings")]
pub struct Cli {
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    #[arg(long, value_name = "SECONDS")]
    pub refresh: Option<u64>,

    #[arg(long, value_name = "ID")]
    pub theme: Option<String>,

    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub json: bool,

    #[arg(long)]
    pub no_color: bool,

    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[arg(short = 't', long)]
    pub tui: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Fetch(FetchArgs),
    /// List built-in TUI theme ids and labels.
    Themes,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Path,
    Show,
    SetUrl { url: String },
    SetRefresh { seconds: u64 },
    SetTheme { id: String },
}

#[derive(Debug, Args)]
pub struct FetchArgs {
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Config(#[from] config::ConfigError),

    #[error(transparent)]
    Device(#[from] device::DeviceError),

    #[error(transparent)]
    DeviceUrl(#[from] DeviceUrlError),

    #[error("failed to serialize output")]
    Output(#[from] serde_json::Error),

    #[error(
        "No AirGradient device URL is configured. Run `airgradient-cli config set-url <URL>` or pass `--url <URL>`."
    )]
    MissingServerUrl,

    #[error(transparent)]
    Tui(#[from] tui::runtime::RuntimeError),

    #[error("`--refresh` is only supported with `--tui`.")]
    RefreshRequiresTui,

    #[error("`--tui` starts the dashboard and cannot be combined with a subcommand.")]
    SubcommandUnsupportedWithTui,

    #[error("`--json` only applies to one-shot fetch output; `--tui` does not support it.")]
    JsonUnsupportedForTui,

    #[error("`--json` only applies to one-shot fetch output; config commands do not support it.")]
    JsonUnsupportedForConfig,

    #[error("`--json` only applies to one-shot fetch output; `themes` does not support it.")]
    JsonUnsupportedForThemes,

    #[error("invalid fetch timeout override `{value}`")]
    InvalidFetchTimeoutOverride {
        value: String,
        #[source]
        source: std::num::ParseIntError,
    },
}

pub async fn run(cli: Cli) -> Result<(), CliError> {
    validate_flag_combination(&cli)?;

    // Read once, here, rather than separately down each branch: this function
    // and `main` are the only places that touch the process environment, so
    // everything below is a function of its arguments.
    let fetch_settings = fetch_settings()?;

    if cli.tui {
        let options = tui::runtime::RuntimeOptions {
            config_path: config_path(&cli)?,
            history_path: history_path(),
            record_path: recording_path(),
            url_override: cli.url.clone(),
            refresh_override_secs: cli.refresh,
            theme_override: cli.theme.clone(),
            fetch_settings,
        };

        return tui::runtime::run(options).await.map_err(CliError::from);
    }

    match &cli.command {
        Some(Command::Config { command }) => run_config_command(command, &cli),
        Some(Command::Fetch(fetch)) => {
            run_fetch(&cli, fetch.json || cli.json, fetch_settings).await
        }
        Some(Command::Themes) => run_themes(),
        None => run_fetch(&cli, cli.json, fetch_settings).await,
    }
}

/// Rejects flag combinations that cannot mean anything, before anything runs.
///
/// All of them are checked here, in one place, so the rules can be read as a
/// set rather than found scattered down the branches that happen to trip over
/// them. The `--tui` ones matter most: the dashboard takes over the terminal
/// and never comes back to run a subcommand or print JSON, so
/// `airgradient-cli --tui config path` used to open the dashboard and silently
/// drop the command, hanging any script that expected a path on stdout.
fn validate_flag_combination(cli: &Cli) -> Result<(), CliError> {
    if cli.tui {
        if cli.command.is_some() {
            return Err(CliError::SubcommandUnsupportedWithTui);
        }
        if cli.json {
            return Err(CliError::JsonUnsupportedForTui);
        }

        return Ok(());
    }

    // `--refresh` only means something to a loop that refreshes.
    if cli.refresh.is_some() {
        return Err(CliError::RefreshRequiresTui);
    }

    // `--json` is the one-shot fetch's output format; the other subcommands
    // have no JSON form to switch to.
    if cli.json {
        match cli.command {
            Some(Command::Config { .. }) => return Err(CliError::JsonUnsupportedForConfig),
            Some(Command::Themes) => return Err(CliError::JsonUnsupportedForThemes),
            Some(Command::Fetch(_)) | None => {}
        }
    }

    Ok(())
}

fn run_themes() -> Result<(), CliError> {
    let mut table = comfy_table::Table::new();
    table.set_header(vec!["ID", "Label"]);
    for theme in tui::theme::ALL {
        table.add_row(vec![theme.id, theme.label]);
    }
    println!("{table}");

    Ok(())
}

/// Every config subcommand except `path` ends by reporting whatever the file
/// needed repairing and then printing the resulting config, so the match
/// decides *which* config that is and both of those happen once, below it.
fn run_config_command(command: &ConfigCommand, cli: &Cli) -> Result<(), CliError> {
    let path = config_path(cli)?;

    let (config, warnings) = match command {
        // The odd one out: it reports where the file is without reading it, so
        // it has nothing to print afterwards.
        ConfigCommand::Path => {
            println!("{}", path.display());
            return Ok(());
        }
        ConfigCommand::Show => {
            let display = config::read_display_config(&path)?;
            (display.config, display.warnings)
        }
        ConfigCommand::SetUrl { url } => {
            let update = config::set_url(&path, url)?;
            (update.config, update.warnings)
        }
        ConfigCommand::SetRefresh { seconds } => {
            let update = config::set_refresh_interval(&path, *seconds)?;
            (update.config, update.warnings)
        }
        ConfigCommand::SetTheme { id } => {
            let update = config::set_theme(&path, id)?;
            (update.config, update.warnings)
        }
    };

    // Warnings to stderr before the config to stdout, so a caller piping the
    // JSON somewhere still sees these on the terminal.
    for warning in warnings {
        eprintln!("warning: {warning}");
    }

    println!("{}", serde_json::to_string_pretty(&config)?);

    Ok(())
}

async fn run_fetch(
    cli: &Cli,
    json: bool,
    fetch_settings: device::FetchSettings,
) -> Result<(), CliError> {
    let path = config_path(cli)?;
    let config = effective_config(&path, cli)?;
    let base_url = required_server_url(&config)?;
    let client = fetch_settings.client()?;

    let started = Instant::now();
    let payload = device::fetch_current_measures_with_client(&client, &base_url).await?;
    let fetch_duration = started.elapsed();
    let snapshot = sensors::parse_snapshot(&payload);
    let previous = record_reading(&snapshot);
    let previous = previous.as_ref().map(|reading| &reading.snapshot);
    let metadata = OutputMetadata {
        device_url: Some(base_url.as_str()),
        last_update: None,
        fetch_duration: Some(fetch_duration),
    };

    if json {
        println!(
            "{}",
            output::json::render_pretty(&snapshot, previous, metadata)?
        );
    } else {
        let no_color = cli.no_color || !io::stdout().is_terminal();
        print!(
            "{}",
            output::text::render(&snapshot, previous, metadata, no_color)
        );
    }

    Ok(())
}

/// Records a one-shot reading alongside the dashboard's own, so a device polled
/// from `watch` or a cron job builds the same history the chart draws from, and
/// returns the reading this one displaces.
///
/// That returned reading is what the Trend column is measured against, so a
/// one-shot fetch can say which way each value is moving. It is the previously
/// recorded reading rather than a remembered one because a one-shot run has no
/// earlier reading of its own — the history file is the only memory it has.
/// The trend is therefore empty on the first run, and whenever recording is
/// switched off.
///
/// Best-effort and silent throughout: this is a side benefit of a fetch, not
/// the point of one. A full disk or an unreadable history costs the trend
/// column, never the reading the user asked for.
fn record_reading(snapshot: &sensors::SensorSnapshot) -> Option<history::Reading> {
    let path = recording_path()?;

    // Read before appending, or the "previous" reading would be this one.
    let previous = history::load_recent(&path, 1)
        .ok()
        .and_then(|mut readings| readings.pop());

    let _ = history::append(
        &path,
        &history::Reading::new(SystemTime::now(), snapshot.clone()),
    );

    previous
}

/// Where the reading history lives, for *reading* it.
///
/// `None` when no data directory can be resolved. Deliberately not an error: a
/// machine with neither `XDG_DATA_HOME` nor `HOME` set can still fetch and
/// display readings, and refusing to start the dashboard over a missing chart
/// would be the wrong trade.
fn history_path() -> Option<PathBuf> {
    history::resolve_history_path(None).ok()
}

/// Where new readings are recorded, or `None` to record nothing.
///
/// The same file as [`history_path`], minus the callers that must not add to
/// it. `AIRGRADIENT_CLI_NO_HISTORY` opts out here rather than at each call
/// site, which is what makes it cover the dashboard as well as a one-shot
/// fetch — the dashboard being by far the busier writer.
///
/// Deliberately does *not* gate reading. Replaying an existing history is a
/// read-only act, so a run that has opted out of recording still opens its
/// chart on whatever is already there.
fn recording_path() -> Option<PathBuf> {
    if env::var_os(NO_HISTORY_ENV).is_some() {
        return None;
    }

    history_path()
}

fn config_path(cli: &Cli) -> Result<PathBuf, CliError> {
    Ok(config::resolve_config_path(cli.config.as_deref())?)
}

fn effective_config(path: &Path, cli: &Cli) -> Result<Config, CliError> {
    let mut config = config::read_config(path)?;

    if let Some(url) = &cli.url {
        config.server_url = Some(DeviceUrl::parse(url)?.as_str().to_owned());
    }

    Ok(config)
}

fn fetch_settings() -> Result<device::FetchSettings, CliError> {
    let Ok(value) = env::var("AIRGRADIENT_CLI_FETCH_TIMEOUT_MS") else {
        return Ok(device::FetchSettings::default());
    };

    let millis = value
        .parse::<u64>()
        .map_err(|source| CliError::InvalidFetchTimeoutOverride { value, source })?;

    Ok(device::FetchSettings::new(Duration::from_millis(millis)))
}

/// The device to fetch from, as a validated address.
///
/// A blank or whitespace-only stored value means "unset" rather than "invalid",
/// so it reports the same missing-config guidance as no value at all.
fn required_server_url(config: &Config) -> Result<DeviceUrl, CliError> {
    config.device_url()?.ok_or(CliError::MissingServerUrl)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes the tests that set process-wide environment variables.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Runs `body` with `AIRGRADIENT_CLI_NO_HISTORY` set or cleared, restoring
    /// whatever was there before.
    fn with_opt_out<T>(enabled: bool, body: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let previous = env::var_os(NO_HISTORY_ENV);

        // SAFETY: the lock above makes this the only thread touching the
        // environment, and the value is restored before it is released.
        unsafe {
            if enabled {
                env::set_var(NO_HISTORY_ENV, "1");
            } else {
                env::remove_var(NO_HISTORY_ENV);
            }
        }

        let result = body();

        unsafe {
            match previous {
                Some(value) => env::set_var(NO_HISTORY_ENV, value),
                None => env::remove_var(NO_HISTORY_ENV),
            }
        }

        result
    }

    #[test]
    fn the_history_opt_out_stops_recording_without_stopping_reading() {
        // Skip where no data directory resolves at all; then both are `None`
        // for a reason unrelated to the opt-out.
        if with_opt_out(false, history_path).is_none() {
            return;
        }

        with_opt_out(true, || {
            assert_eq!(
                recording_path(),
                None,
                "the opt-out must stop new readings being recorded"
            );
            assert!(
                history_path().is_some(),
                "replaying an existing history is read-only, so the opt-out \
                 must not hide it — otherwise the chart opens blank for users \
                 who only asked not to add to it"
            );
        });
    }

    #[test]
    fn without_the_opt_out_readings_are_recorded_where_they_are_read_from() {
        with_opt_out(false, || {
            assert_eq!(
                recording_path(),
                history_path(),
                "one file: what is recorded is what is replayed"
            );
        });
    }
}
