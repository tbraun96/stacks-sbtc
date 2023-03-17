use bitcoin::psbt::serialize::Serialize;
use frost_coordinator::{coordinator::Error as FrostCoordinatorError, create_coordinator};
use frost_signer::net::{Error as HttpNetError, HttpNetListen};
use std::sync::mpsc;
use std::sync::mpsc::Sender;
use std::{thread, time};
use wtfrost::{bip340::SchnorrProof, common::Signature, Point};

use crate::config::{Config, Error as ConfigError};
use crate::peg_wallet::{BitcoinWallet, Error as PegWalletError, PegWallet};
use crate::peg_wallet::{StacksWallet, WrapPegWallet};
use crate::stacks_node;
// Traits in scope
use crate::bitcoin_node::{BitcoinNode, LocalhostBitcoinNode};
use crate::peg_queue::{
    Error as PegQueueError, PegQueue, SbtcOp, SqlitePegQueue, SqlitePegQueueError,
};
use crate::stacks_node::StacksNode;

use crate::stacks_node::client::NodeClient;

type FrostCoordinator = frost_coordinator::coordinator::Coordinator<HttpNetListen>;

// TODO: Define these types
pub type PublicKey = Point;

/// Helper that uses this module's error type
pub type Result<T> = std::result::Result<T, Error>;

/// Kinds of common errors used by stacks coordinator
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Error occurred with the HTTP Relay
    #[error("Http Network Error: {0}")]
    HttpNetError(#[from] HttpNetError),
    /// Error occurred reading Config
    #[error("Config Error: {0}")]
    ConfigError(#[from] ConfigError),
    /// Error occurred in the Peg Queue
    #[error("Peg Queue Error: {0}")]
    PegQueueError(#[from] PegQueueError),
    /// Error occurred in the Peg Wallet
    #[error("Peg Wallet Error: {0}")]
    PegWalletError(#[from] PegWalletError),
    /// Error occurred in the Frost Coordinator
    #[error("Frost Coordinator Error: {0}")]
    FrostCoordinatorError(#[from] FrostCoordinatorError),
    /// Error occurred in the Sqlite Peg Queue
    #[error("Sqlite Peg Queue Error: {0}")]
    SqlitePegQueueError(#[from] SqlitePegQueueError),
}

pub trait Coordinator: Sized {
    type PegQueue: PegQueue;
    type FeeWallet: PegWallet;
    type StacksNode: StacksNode;
    type BitcoinNode: BitcoinNode;

    // Required methods
    fn peg_queue(&self) -> &Self::PegQueue;
    fn fee_wallet(&mut self) -> &mut Self::FeeWallet;
    fn frost_coordinator(&self) -> &FrostCoordinator;
    fn frost_coordinator_mut(&mut self) -> &mut FrostCoordinator;
    fn stacks_node(&self) -> &Self::StacksNode;
    fn bitcoin_node(&self) -> &Self::BitcoinNode;

    // Provided methods
    fn run(mut self) -> Result<()> {
        let (sender, receiver) = mpsc::channel::<Command>();
        Self::poll_ping_thread(sender);

        loop {
            match receiver.recv().expect("thread receive err {0}") {
                Command::Stop => break,
                Command::Timeout => {
                    self.peg_queue()
                        .poll(self.stacks_node())
                        .expect("peg_queue poll error {0}");
                    self.process_queue().expect("peg queue error {0}");
                }
            }
        }
        Ok(())
    }

    fn poll_ping_thread(sender: Sender<Command>) {
        thread::spawn(move || loop {
            sender
                .send(Command::Timeout)
                .expect("thread send error {0}");
            thread::sleep(time::Duration::from_millis(500));
        });
    }

    fn process_queue(&mut self) -> Result<()> {
        match self.peg_queue().sbtc_op()? {
            Some(SbtcOp::PegIn(op)) => self.peg_in(op),
            Some(SbtcOp::PegOutRequest(op)) => self.peg_out(op),
            None => Ok(()),
        }
    }
}

// Private helper functions
trait CoordinatorHelpers: Coordinator {
    fn peg_in(&mut self, op: stacks_node::PegInOp) -> Result<()> {
        let _tx = self.fee_wallet().stacks_mut().mint(&op)?;
        //self.stacks_node().broadcast_transaction(&tx);
        Ok(())
    }

    fn peg_out(&mut self, op: stacks_node::PegOutRequestOp) -> Result<()> {
        let _stacks = self.fee_wallet().stacks_mut();
        let _burn_tx = self.fee_wallet().stacks_mut().burn(&op)?;
        let fulfill_tx = self.fee_wallet().bitcoin_mut().fulfill_peg_out(&op)?;

        //TODO: what do we do with the returned signature?
        self.frost_coordinator_mut()
            .sign_message(&fulfill_tx.serialize())?;

        //self.stacks_node().broadcast_transaction(&burn_tx);
        self.bitcoin_node().broadcast_transaction(&fulfill_tx);
        Ok(())
    }
}

impl<T: Coordinator> CoordinatorHelpers for T {}

pub enum Command {
    Stop,
    Timeout,
}

pub struct StacksCoordinator {
    _config: Config,
    frost_coordinator: FrostCoordinator,
    local_peg_queue: SqlitePegQueue,
    local_stacks_node: NodeClient,
}

impl StacksCoordinator {
    pub fn run_dkg_round(&mut self) -> Result<PublicKey> {
        Ok(self.frost_coordinator.run_distributed_key_generation()?)
    }

    pub fn sign_message(&mut self, message: &str) -> Result<(Signature, SchnorrProof)> {
        Ok(self.frost_coordinator.sign_message(message.as_bytes())?)
    }
}

impl TryFrom<Config> for StacksCoordinator {
    type Error = Error;
    fn try_from(config: Config) -> Result<Self> {
        let stacks_rpc_url = config.stacks_node_rpc_url.clone();
        Ok(Self {
            frost_coordinator: create_coordinator(config.signer_config_path.clone())?,
            _config: config,
            local_peg_queue: SqlitePegQueue::in_memory(0)?,
            local_stacks_node: NodeClient::new(&stacks_rpc_url),
        })
    }
}

impl Coordinator for StacksCoordinator {
    type PegQueue = SqlitePegQueue;
    type FeeWallet = WrapPegWallet;
    type StacksNode = NodeClient;
    type BitcoinNode = LocalhostBitcoinNode;

    fn peg_queue(&self) -> &Self::PegQueue {
        &self.local_peg_queue
    }

    fn fee_wallet(&mut self) -> &mut Self::FeeWallet {
        todo!()
    }

    fn frost_coordinator(&self) -> &FrostCoordinator {
        todo!()
    }

    fn frost_coordinator_mut(&mut self) -> &mut FrostCoordinator {
        todo!()
    }

    fn stacks_node(&self) -> &Self::StacksNode {
        &self.local_stacks_node
    }

    fn bitcoin_node(&self) -> &Self::BitcoinNode {
        todo!()
    }
}

#[cfg(test)]
mod tests {}
