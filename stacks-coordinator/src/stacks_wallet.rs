use crate::{
    peg_wallet::{Error as PegWalletError, StacksWallet as StacksWalletTrait},
    stacks_node::{PegInOp, PegOutRequestOp},
};
use bitcoin::Address as BitcoinAddress;
use blockstack_lib::{
    address::{
        AddressHashMode, C32_ADDRESS_VERSION_MAINNET_SINGLESIG,
        C32_ADDRESS_VERSION_TESTNET_SINGLESIG,
    },
    chainstate::stacks::{
        address::PoxAddress, StacksTransaction, StacksTransactionSigner, TransactionAnchorMode,
        TransactionAuth, TransactionContractCall, TransactionPayload, TransactionSpendingCondition,
        TransactionVersion,
    },
    codec::Error as CodecError,
    core::{CHAIN_ID_MAINNET, CHAIN_ID_TESTNET},
    net::Error as NetError,
    types::{
        chainstate::{StacksAddress, StacksPrivateKey, StacksPublicKey},
        Address,
    },
    util::HexError,
    vm::{
        errors::{Error as ClarityError, RuntimeErrorType},
        types::{ASCIIData, StacksAddressExtensions},
        ClarityName, ContractName, Value,
    },
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("type conversion error from blockstack::bitcoin to bitcoin:: {0}")]
    ConversionError(#[from] bitcoin::hashes::Error),
    ///An invalid contract was specified in the config file
    #[error("Invalid contract name and address: {0}")]
    InvalidContract(String),
    ///An invalid peg out
    #[error("Invalid peg wallet address: {0}")]
    InvalidAddress(PoxAddress),
    ///An invalid public key
    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),
    #[error("Invalid private key: {0}")]
    InvalidPrivateKey(String),
    #[error("Failed to sign transaction.")]
    SigningError,
    #[error("Hex error: {0}")]
    ConversionErrorHex(#[from] HexError),
    #[error("Stacks network error: {0}")]
    NetworkError(#[from] NetError),
    #[error("Clarity runtime error: {0}")]
    ClarityRuntimeError(#[from] RuntimeErrorType),
    #[error("Clarity error: {0}")]
    ClarityGeneralError(#[from] ClarityError),
    #[error("Stacks Code error: {0}")]
    StacksCodeError(#[from] CodecError),
    #[error("Invalid peg-out request op: {0}")]
    InvalidPegOutRequestOp(String),
    #[error("Failed to recover sBTC address from peg-out request op")]
    RecoverError,
}

pub struct StacksWallet {
    contract_address: StacksAddress,
    contract_name: String,
    sender_key: StacksPrivateKey,
    version: TransactionVersion,
    address: StacksAddress,
}

impl StacksWallet {
    pub fn new(
        contract: String,
        sender_key: &str,
        version: TransactionVersion,
    ) -> Result<Self, Error> {
        let sender_key = StacksPrivateKey::from_hex(sender_key)
            .map_err(|e| Error::InvalidPrivateKey(e.to_string()))?;

        let pk = StacksPublicKey::from_private(&sender_key);
        let addr_version = match version {
            TransactionVersion::Mainnet => C32_ADDRESS_VERSION_MAINNET_SINGLESIG,
            TransactionVersion::Testnet => C32_ADDRESS_VERSION_TESTNET_SINGLESIG,
        };

        let address = StacksAddress::from_public_keys(
            addr_version,
            &AddressHashMode::SerializeP2PKH,
            1,
            &vec![pk],
        )
        .ok_or(Error::InvalidPrivateKey(
            "Failed to generate address from public key".to_string(),
        ))
        .map_err(Error::from)?;

        let contract_info: Vec<&str> = contract.split('.').collect();
        if contract_info.len() != 2 {
            return Err(Error::InvalidContract(contract));
        }

        let contract_address = StacksAddress::from_string(contract_info[0]).ok_or(
            Error::InvalidContract("Failed to parse contract address".to_string()),
        )?;
        Ok(Self {
            contract_address,
            contract_name: contract_info[1].to_owned(),
            sender_key,
            version,
            address,
        })
    }

    fn build_transaction_signed(
        &self,
        function_name: impl Into<String>,
        function_args: Vec<Value>,
        nonce: u64,
    ) -> Result<StacksTransaction, Error> {
        // First build an unsigned transaction
        let unsigned_tx = self.build_transaction_unsigned(function_name, function_args, nonce)?;

        // Do the signing
        let mut tx_signer = StacksTransactionSigner::new(&unsigned_tx);
        tx_signer.sign_origin(&self.sender_key)?;

        // Retrieve the signed transaction from the signer
        let signed_tx = tx_signer.get_tx().ok_or(Error::SigningError)?;
        Ok(signed_tx)
    }

    fn build_transaction_unsigned(
        &self,
        function_name: impl Into<String>,
        function_args: Vec<Value>,
        nonce: u64,
    ) -> Result<StacksTransaction, Error> {
        // First build the payload from the provided function and its arguments
        let payload = self.build_transaction_payload(function_name, function_args)?;

        // Next build the authorization from the provided sender key
        let public_key = StacksPublicKey::from_private(&self.sender_key);
        let mut spending_condition = TransactionSpendingCondition::new_singlesig_p2pkh(public_key)
            .ok_or_else(|| {
                Error::InvalidPublicKey(
                    "Failed to create single sig transaction spending condition.".to_string(),
                )
            })?;
        spending_condition.set_nonce(nonce);
        spending_condition.set_tx_fee(0);
        let auth = TransactionAuth::Standard(spending_condition);

        // Viola! We have an unsigned transaction
        let mut tx = StacksTransaction::new(self.version, auth, payload);
        let chain_id = if self.version == TransactionVersion::Testnet {
            CHAIN_ID_TESTNET
        } else {
            CHAIN_ID_MAINNET
        };
        tx.chain_id = chain_id;
        tx.anchor_mode = TransactionAnchorMode::Any;

        Ok(tx)
    }

    fn build_transaction_payload(
        &self,
        function_name: impl Into<String>,
        function_args: Vec<Value>,
    ) -> Result<TransactionPayload, Error> {
        let contract_name = ContractName::try_from(self.contract_name.clone())?;
        let function_name = ClarityName::try_from(function_name.into())?;
        let payload = TransactionContractCall {
            address: self.contract_address,
            contract_name,
            function_name,
            function_args,
        };
        Ok(payload.into())
    }
}

impl StacksWalletTrait for StacksWallet {
    fn build_mint_transaction(
        &mut self,
        op: &PegInOp,
        nonce: u64,
    ) -> Result<StacksTransaction, PegWalletError> {
        let function_name = "mint!";

        // Build the function arguments
        let amount = Value::UInt(op.amount.into());
        let principal = Value::from(op.recipient.clone());
        //Note that this tx_id is only used to print info in the contract call.
        let tx_id = Value::from(ASCIIData {
            data: op.txid.as_bytes().to_vec(),
        });
        let function_args: Vec<Value> = vec![amount, principal, tx_id];
        let tx = self.build_transaction_signed(function_name, function_args, nonce)?;
        Ok(tx)
    }

    fn build_burn_transaction(
        &mut self,
        op: &PegOutRequestOp,
        nonce: u64,
    ) -> Result<StacksTransaction, PegWalletError> {
        let function_name = "burn!";

        // Build the function arguments
        let amount = Value::UInt(op.amount.into());
        // Retrieve the stacks address to burn from
        let address = op
            .stx_address(self.version as u8)
            .map_err(|_| Error::RecoverError)?;
        let principal_data = address.to_account_principal();
        let principal = Value::Principal(principal_data);
        //Note that this tx_id is only used to print info inside the contract call.
        let tx_id = Value::from(ASCIIData {
            data: op.txid.to_bytes().to_vec(),
        });
        let function_args: Vec<Value> = vec![amount, principal, tx_id];

        let tx = self.build_transaction_signed(function_name, function_args, nonce)?;
        Ok(tx)
    }

    fn build_set_btc_address_transaction(
        &mut self,
        address: &BitcoinAddress,
        nonce: u64,
    ) -> Result<StacksTransaction, PegWalletError> {
        let function_name = "set-bitcoin-wallet-address";
        // Build the function arguments
        let address = Value::from(ASCIIData {
            data: address.to_string().into_bytes(),
        });
        let function_args = vec![address];
        let tx = self.build_transaction_signed(function_name, function_args, nonce)?;
        Ok(tx)
    }

    fn address(&self) -> &StacksAddress {
        &self.address
    }
}

#[cfg(test)]
mod tests {
    use crate::{peg_wallet::StacksWallet as StacksWalletTrait, stacks_wallet::StacksWallet};
    use bitcoin::{secp256k1::Secp256k1, XOnlyPublicKey};
    use blockstack_lib::{
        burnchains::{
            bitcoin::{
                address::{BitcoinAddress, SegwitBitcoinAddress},
                BitcoinTransaction, BitcoinTxInput, BitcoinTxOutput,
            },
            BurnchainBlockHeader, BurnchainTransaction, PrivateKey, Txid,
        },
        chainstate::{
            burn::{
                operations::{PegInOp, PegOutRequestOp},
                Opcodes,
            },
            stacks::{address::PoxAddress, TransactionVersion},
        },
        types::chainstate::{BurnchainHeaderHash, StacksAddress, StacksPrivateKey},
        util::hash::{Hash160, Sha256Sum},
        vm::types::{PrincipalData, StandardPrincipalData},
    };
    use rand::Rng;
    use std::str::FromStr;

    fn pox_address() -> PoxAddress {
        PoxAddress::Standard(StacksAddress::new(0, Hash160::from_data(&[0; 20])), None)
    }

    fn stacks_wallet() -> StacksWallet {
        StacksWallet::new(
            "SP3FBR2AGK5H9QBDH3EEN6DF8EK8JY7RX8QJ5SVTE.sbtc-alpha".to_string(),
            &"b244296d5907de9864c0b0d51f98a13c52890be0404e83f273144cd5b9960eed01".to_string(),
            TransactionVersion::Mainnet,
        )
        .unwrap()
    }

    fn build_empty_block_header() -> BurnchainBlockHeader {
        BurnchainBlockHeader {
            block_height: 0,
            block_hash: [0; 32].into(),
            parent_block_hash: [0; 32].into(),
            num_txs: 0,
            timestamp: 0,
        }
    }

    fn build_burnchain_transaction(
        opcode: u8,
        data: Vec<u8>,
        inputs: Vec<BitcoinTxInput>,
        outputs: Vec<BitcoinTxOutput>,
    ) -> BurnchainTransaction {
        BurnchainTransaction::Bitcoin(BitcoinTransaction {
            txid: Txid([0; 32]),
            vtxindex: 0,
            opcode,
            data,
            data_amt: 0,
            inputs,
            outputs,
        })
    }

    fn build_peg_out_request_op(private_key: StacksPrivateKey) -> PegOutRequestOp {
        let mut rng = rand::thread_rng();

        // Build a dust txo
        let dust_amount = 1;
        let recipient_address_bytes = rng.gen::<[u8; 32]>();
        let output2 = BitcoinTxOutput {
            units: dust_amount,
            address: BitcoinAddress::Segwit(SegwitBitcoinAddress::P2TR(
                true,
                recipient_address_bytes,
            )),
        };

        // Build a fulfillment fee txo
        let peg_wallet_address = rng.gen::<[u8; 32]>();
        let fulfillment_fee = 3;
        let output3 = BitcoinTxOutput {
            units: fulfillment_fee,
            address: BitcoinAddress::Segwit(SegwitBitcoinAddress::P2TR(true, peg_wallet_address)),
        };

        // Generate the message signature by signing the amount and recipient fields
        let amount: u64 = 10;
        let mut script_pubkey = vec![81, 32]; // OP_1 OP_PUSHBYTES_32
        script_pubkey.extend_from_slice(&recipient_address_bytes);

        let mut msg = amount.to_be_bytes().to_vec();
        msg.extend_from_slice(&script_pubkey);

        let signature = private_key
            .sign(Sha256Sum::from_data(&msg).as_bytes())
            .expect("Failed to sign amount and recipient fields.");

        let mut data = vec![];
        data.extend_from_slice(&amount.to_be_bytes());
        data.extend_from_slice(signature.as_bytes());

        let outputs = vec![output2, output3];
        let inputs = vec![];

        // Build the burnchain tx using the above generated data
        let tx = build_burnchain_transaction(Opcodes::PegOutRequest as u8, data, inputs, outputs);

        // Build an empty block header
        let header = build_empty_block_header();

        // use the header and tx to generate a peg out request
        PegOutRequestOp::from_tx(&header, &tx).expect("Failed to construct peg-out request op")
    }

    #[test]
    fn stacks_mint_test() {
        let p = PegInOp {
            recipient: PrincipalData::Standard(StandardPrincipalData(0, [0u8; 20])),
            peg_wallet_address: pox_address(),
            amount: 55155,
            memo: Vec::default(),
            txid: Txid([0u8; 32]),
            vtxindex: 0,
            block_height: 0,
            burn_header_hash: BurnchainHeaderHash([0; 32]),
        };
        let mut wallet = stacks_wallet();
        let tx = wallet
            .build_mint_transaction(&p, 0)
            .expect("Failed to construct mint transaction.");
        tx.verify()
            .expect("build_minttransaction generated a transaction with an invalid signature");
    }

    #[test]
    fn stacks_burn_test() {
        let mut wallet = stacks_wallet();
        let op = build_peg_out_request_op(wallet.sender_key);
        let tx = wallet
            .build_burn_transaction(&op, 0)
            .expect("Failed to construct burn transaction.");
        tx.verify()
            .expect("build_burn_transaction generated a transaction with an invalid signature.");
    }

    #[test]
    fn stacks_build_set_btc_address_transaction() {
        let mut wallet = stacks_wallet();
        let internal_key = XOnlyPublicKey::from_str(
            "cc8a4bc64d897bddc5fbc2f670f7a8ba0b386779106cf1223c6fc5d7cd6fc115",
        )
        .unwrap();
        let secp = Secp256k1::verification_only();
        let address = bitcoin::Address::p2tr(&secp, internal_key, None, bitcoin::Network::Testnet);

        let tx = wallet
            .build_set_btc_address_transaction(&address, 0)
            .expect("Failed to construct a set btc address transaction.");
        tx.verify().expect(
            "build_set_btc_address_transaction generated a transaction with an invalid signature.",
        );
    }
}
