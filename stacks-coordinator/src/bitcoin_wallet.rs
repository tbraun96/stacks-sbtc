use crate::bitcoin_node::BitcoinTransaction;
use crate::peg_wallet::{BitcoinWallet as BitcoinWalletTrait, Error as PegWalletError};
use crate::stacks_node::PegOutRequestOp;
use bitcoin::{
    hashes::{
        hex::{Error as HexError, FromHex, ToHex},
        Error as HashesError, Hash,
    },
    Address as BitcoinAddress, Script,
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("type conversion error from blockstack::bitcoin to bitcoin:: {0}")]
    ConversionError(#[from] HashesError),
    #[error("type conversion error blockstack::bitcoin::hashes:hex {0}")]
    ConversionErrorHex(#[from] HexError),
}

pub struct BitcoinWallet {
    address: BitcoinAddress,
}

impl BitcoinWallet {
    pub fn new(address: BitcoinAddress) -> Self {
        Self { address }
    }
}

fn build_transaction(op: &PegOutRequestOp) -> Result<BitcoinTransaction, Error> {
    let bitcoin_txid = bitcoin::Txid::from_slice(op.txid.as_bytes())?;
    let utxo = bitcoin::OutPoint {
        txid: bitcoin_txid,
        vout: op.vtxindex,
    };
    let peg_out_input = bitcoin::TxIn {
        previous_output: utxo,
        script_sig: Default::default(),
        sequence: Default::default(),
        witness: Default::default(),
    };
    //let p2wpk = bitcoin::Script::new_v0_p2wpkh(&user_address.wpubkey_hash().unwrap());
    let peg_out_output_stx = op.recipient.to_bitcoin_tx_out(op.amount);
    let peg_out_script = Script::from_hex(&peg_out_output_stx.script_pubkey.to_hex())?;
    let peg_out_output = bitcoin::TxOut {
        value: peg_out_output_stx.value,
        script_pubkey: peg_out_script,
    };
    Ok(bitcoin::blockdata::transaction::Transaction {
        version: 0,
        lock_time: bitcoin::PackedLockTime(0),
        input: vec![peg_out_input],
        output: vec![peg_out_output],
    })
}

impl BitcoinWalletTrait for BitcoinWallet {
    type Error = Error;
    fn fulfill_peg_out(&self, op: &PegOutRequestOp) -> Result<BitcoinTransaction, PegWalletError> {
        let tx = build_transaction(op)?;
        Ok(tx)
    }

    fn address(&self) -> &BitcoinAddress {
        &self.address
    }
}

#[cfg(test)]
mod tests {
    use super::BitcoinWallet;
    use crate::peg_wallet::BitcoinWallet as BitcoinWalletTrait;
    use crate::stacks_node::PegOutRequestOp;
    use bitcoin::{secp256k1::Secp256k1, XOnlyPublicKey};
    use blockstack_lib::{
        burnchains::Txid,
        chainstate::stacks::address::{PoxAddress, PoxAddressType20},
        types::chainstate::BurnchainHeaderHash,
        util::secp256k1::MessageSignature,
    };
    use std::str::FromStr;

    #[test]
    fn fulfill_peg_out() {
        let internal_key = XOnlyPublicKey::from_str(
            "cc8a4bc64d897bddc5fbc2f670f7a8ba0b386779106cf1223c6fc5d7cd6fc115",
        )
        .unwrap();
        let secp = Secp256k1::verification_only();
        let address = bitcoin::Address::p2tr(&secp, internal_key, None, bitcoin::Network::Testnet);

        let wallet = BitcoinWallet { address };
        let recipient = PoxAddress::Addr20(true, PoxAddressType20::P2WPKH, [0x01; 20]);
        let peg_wallet_address = PoxAddress::Addr20(true, PoxAddressType20::P2WPKH, [0x01; 20]);
        let req_op = PegOutRequestOp {
            amount: 1000,
            recipient: recipient,
            signature: MessageSignature([0x00; 65]),
            peg_wallet_address: peg_wallet_address,
            fulfillment_fee: 0,
            memo: vec![],
            txid: Txid([0x04; 32]),
            vtxindex: 0,
            block_height: 0,
            burn_header_hash: BurnchainHeaderHash([0x00; 32]),
        };
        let btc_tx = wallet.fulfill_peg_out(&req_op).unwrap();
        assert_eq!(btc_tx.output[0].value, 1000)
    }
}
