use frost_signer::config::Config;
use frost_signer::net::{HttpNet, HttpNetListen};
use frost_signer::signer::{Error as SignerError, Signer as FrostSigner};

#[derive(Clone)]
pub struct Signer {
    frost_signer: FrostSigner,
    //TODO: Are there any StacksSigner specific items or maybe a stacks signer specific config that needs to be wrapped around Config?
}

impl Signer {
    pub fn new(config: Config, id: u32) -> Self {
        Self {
            frost_signer: FrostSigner::new(config, id),
        }
    }

    pub async fn start_p2p_async(&mut self) -> Result<(), SignerError> {
        let net: HttpNet = HttpNet::new(self.frost_signer.config.http_relay_url.clone());
        let net_queue = HttpNetListen::new(net.clone(), vec![]);
        self.frost_signer.start_p2p_async(net_queue).await
    }
}
