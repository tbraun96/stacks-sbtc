pub mod client;

use blockstack_lib::chainstate::burn::operations as burn_ops;
use blockstack_lib::types::chainstate::StacksAddress;

pub use blockstack_lib::chainstate::stacks::StacksTransaction;

/// Kinds of common errors used by stacks coordinator
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("Invalid JSON entry")]
    InvalidJsonEntry,
    #[error("JSON serialization Error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Reqwest Error: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Blockstack Error: {0}")]
    BlockstackError(#[from] blockstack_lib::codec::Error),
}

#[cfg_attr(test, mockall::automock)]
pub trait StacksNode {
    fn get_peg_in_ops(&self, block_height: u64) -> Result<Vec<PegInOp>, Error>;
    fn get_peg_out_request_ops(&self, block_height: u64) -> Result<Vec<PegOutRequestOp>, Error>;
    fn burn_block_height(&self) -> Result<u64, Error>;
    fn next_nonce(&self, addr: StacksAddress) -> Result<u64, Error>;
    fn broadcast_transaction(&self, tx: &StacksTransaction) -> Result<(), Error>;
}

pub type PegInOp = burn_ops::PegInOp;
pub type PegOutRequestOp = burn_ops::PegOutRequestOp;
