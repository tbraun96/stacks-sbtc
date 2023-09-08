use std::{borrow::Cow, str::FromStr};

use async_trait::async_trait;
use bdk::descriptor::calc_checksum;
use bitcoin::{consensus::Encodable, hashes::sha256d::Hash, util::amount::Amount, Txid};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};
use url::Url;

#[async_trait]
pub trait BitcoinNode {
    /// Broadcast the BTC transaction to the bitcoin node
    async fn broadcast_transaction(&self, tx: &BitcoinTransaction) -> Result<Txid, Error>;
    /// Load the Bitcoin wallet from the given address
    async fn load_wallet(&self, address: &bitcoin::Address) -> Result<(), Error>;
    /// Get all utxos from the given address
    async fn list_unspent(&self, address: &bitcoin::Address) -> Result<Vec<UTXO>, Error>;
}

pub type BitcoinTransaction = bitcoin::Transaction;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("RPC Error: {0}")]
    RPCError(String),
    #[error("{0}")]
    InvalidResponseJSON(String),
    #[error("Invalid utxo: {0}")]
    InvalidUTXO(String),
    #[error("Invalid transaction hash")]
    InvalidTxHash,
    #[error("Could not compute descriptor checksum: {0}")]
    DescriptorError(#[from] bdk::descriptor::error::Error),
    #[error("URL Parse error: {0}")]
    UrlParseError(#[from] url::ParseError),
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Error::RPCError(parse_rpc_error(value))
    }
}

#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Serialize, Default, PartialEq, Eq, Clone)]
pub struct UTXO {
    pub txid: String,
    pub vout: u32,
    pub address: String,
    pub label: String,
    pub scriptPubKey: String,
    pub amount: u64,
    pub confirmations: u64,
    pub redeemScript: String,
    pub witnessScript: String,
    pub spendable: bool,
    pub solvable: bool,
    pub reused: bool,
    pub desc: String,
    pub safe: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct Wallet {
    name: String,
    warning: String,
}

pub struct LocalhostBitcoinNode {
    bitcoind_api: Url,
    wallet_name: String,
}

#[async_trait]
impl BitcoinNode for LocalhostBitcoinNode {
    async fn broadcast_transaction(&self, tx: &BitcoinTransaction) -> Result<Txid, Error> {
        let mut tx_bytes: Vec<u8> = vec![];
        tx.consensus_encode(&mut tx_bytes)?;
        let raw_tx = hex::encode(&tx_bytes);

        let result = self
            .call("sendrawtransaction", [&raw_tx])
            .await?
            .as_str()
            .ok_or(Error::InvalidResponseJSON(
                "No transaction hash in sendrawtransaction response".to_string(),
            ))?
            .to_string();

        Ok(Txid::from_hash(
            Hash::from_str(&result).map_err(|_| Error::InvalidTxHash)?,
        ))
    }

    async fn load_wallet(&self, address: &bitcoin::Address) -> Result<(), Error> {
        debug!("Loading bitcoin wallet...");
        let result = self.create_empty_wallet().await;
        if let Err(Error::RPCError(message)) = &result {
            if message.contains("Database already exists") {
                // If the database already exists, no problem.
                info!("Wallet already exists");
            } else {
                return result;
            }
        }
        // Import the address
        self.import_address(address).await?;
        Ok(())
    }

    /// List the UTXOs filtered on a given address.
    async fn list_unspent(&self, address: &bitcoin::Address) -> Result<Vec<UTXO>, Error> {
        debug!("Retrieving utxos...");
        // Construct the params using defaults found at https://developer.bitcoin.org/reference/rpc/listunspent.html?highlight=listunspent
        let addresses: Vec<String> = vec![address.to_string()];
        let min_conf = 0i64;
        let max_conf = 9999999i64;
        let params = (min_conf, max_conf, addresses);

        let response = self.call_wallet("listunspent", params).await?;

        // Convert the response to a vector of unspent transactions
        let result: Result<Vec<UTXO>, Error> = response
            .as_array()
            .ok_or(Error::InvalidResponseJSON(
                "Listunspent response is not an array".to_string(),
            ))?
            .iter()
            .map(Self::raw_to_utxo)
            .collect();

        result
    }
}

impl LocalhostBitcoinNode {
    pub fn new(bitcoind_api: Url) -> LocalhostBitcoinNode {
        Self {
            bitcoind_api,
            wallet_name: "stacks_coordinator".to_string(),
        }
    }

    /// Make the Bitcoin RPC method call with the corresponding paramenters
    async fn call(&self, method: &str, params: impl Serialize) -> Result<serde_json::Value, Error> {
        self.call_path(method, params, None).await
    }

    /// Make the Bitcoin RPC method call against the "/wallet/<self.wallet_name" with the corresponding paramenters
    async fn call_wallet(
        &self,
        method: &str,
        params: impl Serialize,
    ) -> Result<serde_json::Value, Error> {
        self.call_path(
            method,
            params,
            Some(&format!("/wallet/{}", self.wallet_name)),
        )
        .await
    }

    /// Make the Bitcoin RPC method call against the specified path with the corresponding paramenters
    async fn call_path(
        &self,
        method: &str,
        params: impl Serialize,
        path: Option<&str>,
    ) -> Result<serde_json::Value, Error> {
        debug!("Making Bitcoin RPC {} call...", method);
        let json_rpc =
            serde_json::json!({"jsonrpc": "2.0", "id": "stx", "method": method, "params": params});

        let url = if let Some(path) = path {
            Cow::Owned(self.bitcoind_api.join(path)?)
        } else {
            Cow::Borrowed(&self.bitcoind_api)
        };

        let response = reqwest::Client::new()
            .post(&url.to_string())
            .json(&json_rpc)
            .send()
            .await?;

        let json_response = response.json::<serde_json::Value>().await?;
        let json_result = json_response
            .get("result")
            .ok_or_else(|| Error::InvalidResponseJSON("Missing entry 'result'.".to_string()))?
            .to_owned();
        Ok(json_result)
    }

    async fn create_empty_wallet(&self) -> Result<(), Error> {
        debug!("Creating wallet...");
        let wallet_name = &self.wallet_name;
        let disable_private_keys = true;
        let blank = true;
        let passphrase = "";
        let avoid_reuse = false;
        let descriptors = true;
        let load_on_startup = true;
        let params = (
            wallet_name,
            disable_private_keys,
            blank,
            passphrase,
            avoid_reuse,
            descriptors,
            load_on_startup,
        );
        let wallet = serde_json::from_value::<Wallet>(self.call("createwallet", params).await?)
            .map_err(|e| Error::InvalidResponseJSON(e.to_string()))?;
        if !wallet.warning.is_empty() {
            warn!(
                "Wallet {} was not loaded cleanly: {}",
                wallet.name, wallet.warning
            );
        }
        Ok(())
    }

    async fn import_address(&self, address: &bitcoin::Address) -> Result<(), Error> {
        debug!("Importing address {}...", address);

        // Create a descriptor using a Bech32 (segwit) Pay-to-Taproot (P2TR) address.
        let desc = {
            let descriptor = format!("addr({})", address);
            let checksum = calc_checksum(&descriptor)?;

            format!("{}#{}", descriptor, checksum)
        };

        let timestamp = "now";
        // let range = 1; // Set to 1 to ensure only one address is imported
        let internal = false;
        let watchonly = true;
        let label = "";
        let keypool = true;
        let rescan = true;
        let descriptor_object = serde_json::json!({
            "desc": desc,
            // "range": range,
            "timestamp": timestamp,
            "internal": internal,
            "watchonly": watchonly,
            "label": label,
            "keypool": keypool,
            "rescan": rescan
        });
        let params = (serde_json::json!([descriptor_object]),);
        self.call_wallet("importdescriptors", params).await?;
        Ok(())
    }

    fn raw_to_utxo(raw: &Value) -> Result<UTXO, Error> {
        Ok(UTXO {
            txid: raw["txid"]
                .as_str()
                .ok_or(Error::InvalidResponseJSON(
                    "Could not parse txid".to_string(),
                ))?
                .to_string(),
            vout: raw["vout"].as_u64().ok_or(Error::InvalidResponseJSON(
                "Could not parse vout".to_string(),
            ))? as u32,
            address: raw["address"]
                .as_str()
                .ok_or(Error::InvalidResponseJSON(
                    "Could not parse address".to_string(),
                ))?
                .to_string(),
            label: raw["label"]
                .as_str()
                .ok_or(Error::InvalidResponseJSON(
                    "Could not parse label".to_string(),
                ))?
                .to_string(),
            scriptPubKey: raw["scriptPubKey"]
                .as_str()
                .ok_or(Error::InvalidResponseJSON(
                    "Could not parse scriptPubKey".to_string(),
                ))?
                .to_string(),
            amount: raw["amount"]
                .as_f64()
                .map_or_else(
                    || {
                        Err(Error::InvalidResponseJSON(
                            "Amount not provided or wrong type".to_string(),
                        ))
                    },
                    |amount| {
                        Amount::from_btc(amount).map_err(|_e| {
                            Error::InvalidResponseJSON(format!(
                                "Could not parse the float {} as a bitcoin amount",
                                amount
                            ))
                        })
                    },
                )?
                .to_sat(),
            confirmations: raw["confirmations"]
                .as_u64()
                .ok_or(Error::InvalidResponseJSON(
                    "Could not parse confirmations".to_string(),
                ))?,
            redeemScript: "".to_string(),
            witnessScript: "".to_string(),
            spendable: raw["spendable"]
                .as_bool()
                .ok_or(Error::InvalidResponseJSON(
                    "Could not parse spendable".to_string(),
                ))?,
            solvable: raw["solvable"].as_bool().ok_or(Error::InvalidResponseJSON(
                "Could not parse solvable".to_string(),
            ))?,
            reused: false,
            desc: "".to_string(),
            safe: raw["safe"].as_bool().ok_or(Error::InvalidResponseJSON(
                "Could not parse safe".to_string(),
            ))?,
        })
    }
}

fn parse_rpc_error(err: reqwest::Error) -> String {
    format!("{} {}", err.status().unwrap_or_default(), err)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn should_map_json_to_utxo() {
        let value = json!({
            "address": "bcrt1qykqup0h6ry9x3c89llzpznrvm9nfd7fqwnt0hu",
            "amount": 0.00000050,
            "confirmations": 123,
            "label": "",
            "parent_descs": [],
            "safe": true,
            "scriptPubKey": "00142581c0befa190a68e0e5ffc4114c6cd96696f920",
            "solvable": false,
            "spendable": false,
            "txid": "19b7fb5fd6dc25b76aeedb812b7fdc7bf8fac343913706c8b39d23ef7375860c",
            "vout": 0,
        });

        let res = LocalhostBitcoinNode::raw_to_utxo(&value).unwrap();

        assert_eq!(
            res,
            UTXO {
                txid: "19b7fb5fd6dc25b76aeedb812b7fdc7bf8fac343913706c8b39d23ef7375860c"
                    .to_string(),
                vout: 0,
                address: "bcrt1qykqup0h6ry9x3c89llzpznrvm9nfd7fqwnt0hu".to_string(),
                label: "".to_string(),
                scriptPubKey: "00142581c0befa190a68e0e5ffc4114c6cd96696f920".to_string(),
                amount: 50,
                confirmations: 123,
                redeemScript: "".to_string(),
                witnessScript: "".to_string(),
                spendable: false,
                solvable: false,
                reused: false,
                desc: "".to_string(),
                safe: true,
            }
        );
    }
}
