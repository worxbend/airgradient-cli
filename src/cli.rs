use std::{
    path::{Path, PathBuf},
    time::Instant,
};

use clap::{Args, Parser, Subcommand};
use thiserror::Error;

use crate::{
    config::{self, Config},
    device,
    output::{self, OutputMetadata},
    sensors,
};

#[derive(Debug, Parser)]
#[command(version, about = "Fetch and render AirGradient sensor readings")]
pub struct Cli {
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    #[arg(long, value_name = "SECONDS")]
    pub refresh: Option<u64>,

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
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Path,
    Show,
    SetUrl { url: String },
    SetRefresh { seconds: u64 },
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

    #[error("TUI is not implemented yet.")]
    TuiNotImplemented,
}

pub async fn run(cli: Cli) -> Result<(), CliError> {
    if cli.tui {
        return Err(CliError::TuiNotImplemented);
    }

    match &cli.command {
        Some(Command::Config { command }) => run_config_command(command, &cli).await,
        Some(Command::Fetch(fetch)) => run_fetch(&cli, fetch.json || cli.json).await,
        None => run_fetch(&cli, cli.json).await,
    }
}

async fn run_config_command(command: &ConfigCommand, cli: &Cli) -> Result<(), CliError> {
    let path = config_path(cli)?;

    match command {
        ConfigCommand::Path => {
            println!("{}", path.display());
        }
        ConfigCommand::Show => {
            let config = config::read_config(&path)?;
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        ConfigCommand::SetUrl { url } => {
            let config = config::set_url(&path, url)?;
            println!("{}", serde_json::to_string_pretty(&config)?);
        }
        ConfigCommand::SetRefresh { seconds } => {
            let config = config::set_refresh_interval(&path, *seconds)?;
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

    let started = Instant::now();
    let payload = device::fetch_current_measures(&base_url).await?;
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
        print!(
            "{}",
            output::text::render(&snapshot, None, metadata, cli.no_color)
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

    if let Some(refresh) = cli.refresh {
        config::validate_refresh_interval(refresh)?;
        config.refresh_interval_secs = refresh;
    }

    Ok(config)
}

fn required_server_url(config: &Config) -> Result<String, CliError> {
    config
        .server_url
        .as_deref()
        .filter(|url| !url.trim().is_empty())
        .map(str::to_owned)
        .ok_or(CliError::MissingServerUrl)
}
