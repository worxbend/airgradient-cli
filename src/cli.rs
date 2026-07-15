use std::{
    env,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use clap::{Args, Parser, Subcommand};
use thiserror::Error;

use crate::{
    config::{self, Config},
    device,
    output::{self, OutputMetadata},
    sensors, tui,
};

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
    if cli.tui {
        let options = tui::runtime::RuntimeOptions {
            config_path: config_path(&cli)?,
            url_override: cli.url.clone(),
            refresh_override_secs: cli.refresh,
            theme_override: cli.theme.clone(),
            fetch_settings: fetch_settings()?,
        };

        return tui::runtime::run(options).await.map_err(CliError::from);
    }

    if cli.refresh.is_some() {
        return Err(CliError::RefreshRequiresTui);
    }

    match &cli.command {
        Some(Command::Config { command }) => run_config_command(command, &cli).await,
        Some(Command::Fetch(fetch)) => run_fetch(&cli, fetch.json || cli.json).await,
        Some(Command::Themes) => run_themes(&cli),
        None => run_fetch(&cli, cli.json).await,
    }
}

fn run_themes(cli: &Cli) -> Result<(), CliError> {
    if cli.json {
        return Err(CliError::JsonUnsupportedForThemes);
    }

    let mut table = comfy_table::Table::new();
    table.set_header(vec!["ID", "Label"]);
    for theme in tui::theme::ALL {
        table.add_row(vec![theme.id, theme.label]);
    }
    println!("{table}");

    Ok(())
}

async fn run_config_command(command: &ConfigCommand, cli: &Cli) -> Result<(), CliError> {
    if cli.json {
        return Err(CliError::JsonUnsupportedForConfig);
    }

    let path = config_path(cli)?;

    match command {
        ConfigCommand::Path => {
            println!("{}", path.display());
        }
        ConfigCommand::Show => {
            let display = config::read_display_config(&path)?;
            for warning in display.warnings {
                eprintln!("warning: {warning}");
            }
            println!("{}", serde_json::to_string_pretty(&display.config)?);
        }
        ConfigCommand::SetUrl { url } => {
            let config = config::set_url(&path, url)?;
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        ConfigCommand::SetRefresh { seconds } => {
            let config = config::set_refresh_interval(&path, *seconds)?;
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        ConfigCommand::SetTheme { id } => {
            let config = config::set_theme(&path, id)?;
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
    }

    Ok(())
}

async fn run_fetch(cli: &Cli, json: bool) -> Result<(), CliError> {
    let path = config_path(cli)?;
    let config = effective_config(&path, cli)?;
    let server_url = required_server_url(&config)?;
    let base_url = device::normalize_base_url(&server_url)?;
    let fetch_settings = fetch_settings()?;
    let client = fetch_settings.client()?;

    let started = Instant::now();
    let payload = device::fetch_current_measures_with_client(&client, &base_url).await?;
    let fetch_duration = started.elapsed();
    let snapshot = sensors::parse_snapshot(&payload);
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

fn config_path(cli: &Cli) -> Result<PathBuf, CliError> {
    Ok(config::resolve_config_path(cli.config.as_deref())?)
}

fn effective_config(path: &Path, cli: &Cli) -> Result<Config, CliError> {
    let mut config = config::read_config(path)?;

    if let Some(url) = &cli.url {
        let normalized = device::normalize_base_url(url)?;
        config.server_url = Some(normalized.to_string());
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

fn required_server_url(config: &Config) -> Result<String, CliError> {
    config
        .server_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
        .map(str::to_owned)
        .ok_or(CliError::MissingServerUrl)
}
