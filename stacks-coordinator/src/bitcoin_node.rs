use crate::bitcoin_node::Error::{RpcMissingResult, RpcResultNotObject};

pub trait BitcoinNode {
    fn broadcast_transaction(&self, tx: &BitcoinTransaction);
}

pub type BitcoinTransaction = bitcoin::Transaction;

pub struct LocalhostBitcoinNode {
    _bitcoind_api: String,
}
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("HTTP Error: {0}")]
    HttpError(#[from] Box<ureq::Error>),
    #[error("RPC Missing result error")]
    RpcMissingResult,
    #[error("RPC result not an object")]
    RpcResultNotObject,
}

impl BitcoinNode for LocalhostBitcoinNode {
    fn broadcast_transaction(&self, _tx: &BitcoinTransaction) {
        let _todo = self.rpc(&self._bitcoind_api, "sendrawtransaction", [""]); // todo
    }
}

impl LocalhostBitcoinNode {
    fn rpc(
        &self,
        url: &str,
        method: &str,
        params: impl ureq::serde::Serialize,
    ) -> Result<serde_json::Value, Error> {
        let rpc = ureq::json!({"jsonrpc": "1.0", "id": "stx", "method": method, "params": params});
        let response = ureq::post(url).send_json(&rpc).map_err(Box::new)?;
        let json = response.into_json::<serde_json::Value>()?;
        let result = json.as_object().ok_or_else(|| RpcResultNotObject)?;
        let result_str = result.get("result").ok_or_else(|| RpcMissingResult)?;
        Ok(result_str.clone())
    }
}
