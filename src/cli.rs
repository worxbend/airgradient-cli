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

/// Every config subcommand except `path` ends by printing the resulting
/// config, so the match decides *which* config that is and the printing
/// happens once, below it.
fn run_config_command(command: &ConfigCommand, cli: &Cli) -> Result<(), CliError> {
    let path = config_path(cli)?;

    let config = match command {
        // The odd one out: it reports where the file is without reading it, so
        // it has nothing to print afterwards.
        ConfigCommand::Path => {
            println!("{}", path.display());
            return Ok(());
        }
        ConfigCommand::Show => {
            let display = config::read_display_config(&path)?;
            for warning in display.warnings {
                eprintln!("warning: {warning}");
            }
            display.config
        }
        ConfigCommand::SetUrl { url } => config::set_url(&path, url)?,
        ConfigCommand::SetRefresh { seconds } => config::set_refresh_interval(&path, *seconds)?,
        ConfigCommand::SetTheme { id } => config::set_theme(&path, id)?,
    };

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
    record_reading(&snapshot);
    let metadata = OutputMetadata {
        device_url: Some(base_url.as_str()),
        last_update: None,
        fetch_duration: Some(fetch_duration),
    };

    if json {
        println!(
            "{}",
            output::json::render_pretty(&snapshot, None, metadata)?
        );
    } else {
        let no_color = cli.no_color || !io::stdout().is_terminal();
        print!(
            "{}",
            output::text::render(&snapshot, None, metadata, no_color)
        );
    }

    Ok(())
}

/// Records a one-shot reading alongside the dashboard's own, so a device polled
/// from `watch` or a cron job builds the same history the chart draws from.
///
/// Best-effort and silent: this is a side benefit of a fetch, not the point of
/// one, and a full disk must not turn a successful reading into a failed
/// command.
fn record_reading(snapshot: &sensors::SensorSnapshot) {
    if let Some(path) = history_path() {
        let _ = history::append(
            &path,
            &history::Reading::new(SystemTime::now(), snapshot.clone()),
        );
    }
}

/// Where readings are recorded, or `None` when nothing should be recorded.
///
/// `None` has two causes, and both mean the same thing to every caller —
/// record nothing:
///
/// - `AIRGRADIENT_CLI_NO_HISTORY` is set, for callers that want a genuinely
///   read-only run. Answering it here rather than at one call site is what
///   makes the opt-out cover the dashboard as well as a one-shot fetch.
/// - No data directory can be resolved. Deliberately not an error: a machine
///   with neither `XDG_DATA_HOME` nor `HOME` set can still fetch and display
///   readings, and refusing to start the dashboard over a missing chart would
///   be the wrong trade.
fn history_path() -> Option<PathBuf> {
    if env::var_os(NO_HISTORY_ENV).is_some() {
        return None;
    }

    history::resolve_history_path(None).ok()
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
