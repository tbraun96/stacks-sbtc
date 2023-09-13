use clap::Parser;
use tracing::{error, info, warn};

use frost_signer::config::{Cli, Config};
use frost_signer::logging;
use frost_signer::net::{HttpNet, HttpNetListen};
use frost_signer::signer::Signer;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    logging::initiate_tracing_subscriber();

    let cli = Cli::parse();

    match Config::from_path(&cli.config) {
        Ok(config) => {
            let mut signer = Signer::new(config, cli.id);
            let net: HttpNet = HttpNet::new(signer.config.http_relay_url.clone());
            let net_queue = HttpNetListen::new(net.clone(), vec![]);
            info!(
                "{} signer id #{}",
                frost_signer::version(),
                signer.signer_id
            ); // sign-on message

            //Start listening for p2p messages
            if let Err(e) = signer.start_p2p_async(net_queue).await {
                warn!("An error occurred in the P2P Network: {}", e);
            }
        }
        Err(e) => {
            error!("An error occrred reading config file {}: {}", cli.config, e);
        }
    }
}
