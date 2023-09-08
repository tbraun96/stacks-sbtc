use clap::Parser;
use frost_signer::logging;
use stacks_coordinator::cli::{Cli, Command};
use stacks_coordinator::config::Config;
use stacks_coordinator::coordinator::{config_to_stacks_coordinator, Coordinator};
use tracing::{error, info, warn};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    logging::initiate_tracing_subscriber();

    //TODO: get configs from sBTC contract
    match Config::from_path(&cli.config) {
        Ok(mut config) => {
            config.signer_config_path = Some(cli.signer_config);
            if cli.start_block_height == Some(0) {
                error!("Invalid start block height. Must specify a value greater than 0.",);
                return;
            }
            config.start_block_height = cli.start_block_height;
            match config_to_stacks_coordinator(&config).await {
                Ok(mut coordinator) => {
                    // Determine what action the caller wishes to perform
                    match cli.command {
                        Command::Run => {
                            info!("Running Coordinator");
                            //TODO: set up coordination with the stacks node
                            if let Err(e) = coordinator.run(config.polling_interval).await {
                                error!("An error occurred running the coordinator: {}", e);
                            }
                        }
                        Command::Dkg => {
                            info!("Running DKG Round");
                            if let Err(e) = coordinator.run_dkg_round().await {
                                error!("An error occurred during DKG round: {}", e);
                            }
                        }
                        Command::DkgSign => {
                            info!("Running DKG Round");
                            if let Err(e) = coordinator.run_dkg_round().await {
                                warn!("An error occurred during DKG round: {}", e);
                            };
                            info!("Running Signing Round");
                            let (signature, schnorr_proof) =
                                match coordinator.sign_message("Hello, world!").await {
                                    Ok((sig, proof)) => (sig, proof),
                                    Err(e) => {
                                        panic!("signing message failed: {e}");
                                    }
                                };
                            info!(
                                "Got good signature ({},{}) and schnorr proof ({},{})",
                                &signature.R, &signature.z, &schnorr_proof.r, &schnorr_proof.s
                            );
                        }
                    };
                }
                Err(e) => {
                    error!("An error occurred creating coordinator: {}", e);
                }
            }
        }
        Err(e) => {
            error!(
                "An error occurred reading config file {}: {}",
                cli.config, e
            );
        }
    }
}
