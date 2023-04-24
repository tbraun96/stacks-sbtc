use crate::stacks_node::{Error as StacksNodeError, PegInOp, PegOutRequestOp, StacksNode};
use blockstack_lib::{
    chainstate::stacks::{address::StacksAddressExtensions, StacksTransaction},
    codec::StacksMessageCodec,
    types::chainstate::StacksAddress,
};
use reqwest::blocking::Client;
use serde_json::Value;
use tracing::debug;

/// Kinds of common errors used by stacks coordinator
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Stacks Node Error: {0}")]
    StacksNodeError(#[from] StacksNodeError),
}

pub struct NodeClient {
    node_url: String,
    client: Client,
}

impl NodeClient {
    pub fn new(url: &str) -> Self {
        Self {
            node_url: url.to_string(),
            client: Client::new(),
        }
    }

    fn build_url(&self, route: &str) -> String {
        format!("{}{}", self.node_url, route)
    }

    fn get_response(&self, route: &str) -> Result<String, StacksNodeError> {
        let url = self.build_url(route);
        debug!("Sending Request to Stacks Node: {}", &url);
        Ok(self.client.get(&url).send()?.text()?)
    }

    fn get_burn_ops<T>(&self, block_height: u64, op: &str) -> Result<Vec<T>, StacksNodeError>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self.get_response(&format!("/v2/burn_ops/{block_height}/{op}"))?;
        let failure_msg = format!("Could not find burn block at height {block_height}");
        if failure_msg == response {
            Err(StacksNodeError::UnknownBlockHeight(block_height))
        } else {
            let json = serde_json::from_str::<Value>(&response)?;
            Ok(serde_json::from_value(json[op].clone())?)
        }
    }
}

impl StacksNode for NodeClient {
    fn get_peg_in_ops(&self, block_height: u64) -> Result<Vec<PegInOp>, StacksNodeError> {
        self.get_burn_ops::<PegInOp>(block_height, "peg_in")
    }

    fn get_peg_out_request_ops(
        &self,
        block_height: u64,
    ) -> Result<Vec<PegOutRequestOp>, StacksNodeError> {
        self.get_burn_ops::<PegOutRequestOp>(block_height, "peg_out_request")
    }

    fn burn_block_height(&self) -> Result<u64, StacksNodeError> {
        let response = self.get_response("/v2/info")?;
        let entry = "burn_block_height";
        let json: Value = serde_json::from_str(&response)?;
        json[entry]
            .as_u64()
            .ok_or_else(|| StacksNodeError::InvalidJsonEntry(entry.to_string()))
    }

    fn next_nonce(&self, addr: &StacksAddress) -> Result<u64, StacksNodeError> {
        let url = self.build_url(&format!("/v2/accounts/{}", addr.to_b58()));
        let entry = "nonce";
        self.client.get(url).send()?.json::<Value>().map(|json| {
            json[entry]
                .as_u64()
                .map(|val| val + 1)
                .ok_or_else(|| StacksNodeError::InvalidJsonEntry(entry.to_string()))
        })?
    }

    fn broadcast_transaction(&self, tx: &StacksTransaction) -> Result<(), StacksNodeError> {
        let url = self.build_url("/v2/transactions");
        let mut buffer = vec![];

        tx.consensus_serialize(&mut buffer)?;
        self.client.post(url).body(buffer).send()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufWriter, Read, Write},
        net::{SocketAddr, TcpListener},
        thread::{sleep, spawn},
        time::Duration,
    };

    use blockstack_lib::{
        chainstate::stacks::{
            CoinbasePayload, SinglesigHashMode, SinglesigSpendingCondition, TransactionAnchorMode,
            TransactionAuth, TransactionPayload, TransactionPostConditionMode,
            TransactionPublicKeyEncoding, TransactionSpendingCondition, TransactionVersion,
        },
        util::{hash::Hash160, secp256k1::MessageSignature},
    };

    use super::*;

    #[test]
    fn should_send_tx_bytes_to_node() {
        let tx = StacksTransaction {
            version: TransactionVersion::Testnet,
            chain_id: 0,
            auth: TransactionAuth::Standard(TransactionSpendingCondition::Singlesig(
                SinglesigSpendingCondition {
                    hash_mode: SinglesigHashMode::P2PKH,
                    signer: Hash160([0; 20]),
                    nonce: 0,
                    tx_fee: 0,
                    key_encoding: TransactionPublicKeyEncoding::Uncompressed,
                    signature: MessageSignature([0; 65]),
                },
            )),
            anchor_mode: TransactionAnchorMode::Any,
            post_condition_mode: TransactionPostConditionMode::Allow,
            post_conditions: vec![],
            payload: TransactionPayload::Coinbase(CoinbasePayload([0; 32]), None),
        };

        let mut tx_bytes = [0u8; 1024];

        {
            let mut tx_bytes_writer = BufWriter::new(&mut tx_bytes[..]);

            tx.consensus_serialize(&mut tx_bytes_writer).unwrap();

            tx_bytes_writer.flush().unwrap();
        }

        let bytes_len = tx_bytes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, &x)| x != 0)
            .unwrap()
            .0
            + 1;

        let mut mock_server_addr = SocketAddr::from(([127, 0, 0, 1], 0));
        let mock_server = TcpListener::bind(mock_server_addr).unwrap();

        mock_server_addr.set_port(mock_server.local_addr().unwrap().port());

        let h = spawn(move || {
            sleep(Duration::from_millis(100));

            let client = NodeClient::new(&format!("http://{}", mock_server_addr));
            client.broadcast_transaction(&tx).unwrap();
        });

        let mut request_bytes = [0u8; 1024];

        {
            let mut stream = mock_server.accept().unwrap().0;

            stream.read(&mut request_bytes).unwrap();
            stream.write("HTTP/1.1 200 OK\n\n".as_bytes()).unwrap();
        }

        h.join().unwrap();

        assert!(
            request_bytes
                .windows(bytes_len)
                .any(|window| window == &tx_bytes[..bytes_len]),
            "Request bytes did not contain the transaction bytes"
        );
    }
}
