use std::{error::Error, process::ExitCode};

use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt};

use airgradient_cli::cli;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    let verbose = cli.verbose;

    init_tracing(cli.verbose);
    match cli::run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            print_error(&error, verbose);
            ExitCode::FAILURE
        }
    }
}

fn print_error(error: &dyn Error, verbose: u8) {
    eprintln!("error: {error}");

    if verbose > 0 && error.source().is_some() {
        eprintln!();
        eprintln!("caused by:");
        let mut source = error.source();
        let mut index = 1;
        while let Some(source_error) = source {
            eprintln!("  {index}: {source_error}");
            source = source_error.source();
            index += 1;
        }
    }

    if verbose > 1 {
        eprintln!();
        eprintln!("debug: {error:?}");
    }
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
