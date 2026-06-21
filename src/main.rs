use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt};

pub mod cli;
pub mod config;
pub mod device;
pub mod output;
pub mod sensors;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let cli = cli::Cli::parse();

    init_tracing(cli.verbose);
    cli::run(cli).await?;

    Ok(())
}

fn init_tracing(verbose: u8) {
    if verbose == 0 {
        return;
    }

    let filter = if verbose > 1 { "trace" } else { "debug" };
    let _ = fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_writer(std::io::stderr)
        .try_init();
}
