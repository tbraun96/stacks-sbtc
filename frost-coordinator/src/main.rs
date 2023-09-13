use clap::Parser;

use frost_coordinator::{coordinator::Command, create_coordinator_from_path};
use frost_signer::logging;
use tracing::{error, warn};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Config file path
    #[arg(short, long)]
    config: String,
    /// Subcommand action to take
    #[command(subcommand)]
    pub command: Command,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    logging::initiate_tracing_subscriber();

    let cli = Cli::parse();
    match create_coordinator_from_path(cli.config) {
        Ok(mut coordinator) => {
            if let Err(e) = coordinator.run(&cli.command).await {
                warn!("Failed to execute command: {}", e);
            }
        }
        Err(e) => {
            error!("Failed to create coordinator: {}", e);
        }
    }
}
