use anyhow::anyhow;
use bdk::blockchain::Blockchain;
use bitcoin::{psbt::serialize::Deserialize, Network, Transaction};
use clap::Parser;

use crate::commands::utils;

#[derive(Parser, Debug, Clone)]
pub struct BroadcastArgs {
    /// The network to broadcast to
    #[clap(short, long, default_value_t = Network::Testnet)]
    network: Network,
    /// The transaction to broadcast
    tx: String,
}

pub fn broadcast_tx(broadcast: &BroadcastArgs) -> anyhow::Result<()> {
    let blockchain = utils::init_blockchain()?;
    let tx = Transaction::deserialize(
        &array_bytes::hex2bytes(&broadcast.tx).map_err(|e| anyhow!("{:?}", e))?,
    )?;
    blockchain.broadcast(&tx)?;

    println!("Broadcasted tx: {}", tx.txid());
    Ok(())
}
