use crate::{
    make_contract_call::{
        Error as ContractError, MakeContractCall, SignedContractCallOptions, ANY,
    },
    peg_wallet::{Error as PegWalletError, PegWalletAddress, StacksWallet as StacksWalletTrait},
    stacks_node::{PegInOp, PegOutRequestOp},
    stacks_transaction::StacksTransaction,
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("type conversion error from blockstack::bitcoin to bitcoin:: {0}")]
    ConversionError(#[from] bitcoin::hashes::Error),
    #[error("type conversion error blockstack::bitcoin::hashes:hex {0}")]
    ConversionErrorHex(#[from] bitcoin::hashes::hex::Error),
    ///Error occurred calling the sBTC contract
    #[error("Contract Error: {0}")]
    ContractError(#[from] ContractError),
    ///An invalid contract was specified in the config file
    #[error("Invalid contract name and address: {0}")]
    InvalidContract(String),
}

pub struct StacksWallet {
    make_contract_call: MakeContractCall,
    contract_address: String,
    contract_name: String,
    sender_key: String,
}

impl StacksWallet {
    pub fn new(path: &str, contract: String, sender_key: String) -> Result<Self, Error> {
        let contract_info: Vec<&str> = contract.split('.').collect();
        if contract_info.len() != 2 {
            return Err(Error::InvalidContract(contract));
        }
        Ok(Self {
            make_contract_call: MakeContractCall::new(path)?,
            contract_address: contract_info[0].to_owned(),
            contract_name: contract_info[1].to_owned(),
            sender_key,
        })
    }
    fn call(&mut self, function_name: String) -> Result<StacksTransaction, Error> {
        let input = SignedContractCallOptions {
            contractAddress: self.contract_address.clone(),
            contractName: self.contract_name.to_string(),
            functionName: function_name,
            functionArgs: Vec::default(),
            fee: Some(0.to_string()),
            feeEstimateApiUrl: None,
            nonce: None,
            network: None,
            anchorMode: ANY,
            postConditionMode: None,
            postConditions: None,
            validateWithAbi: None,
            sponsored: None,
            senderKey: self.sender_key.clone(),
        };
        Ok(self.make_contract_call.call(&input)?)
    }
}

impl StacksWalletTrait for StacksWallet {
    fn build_mint_transaction(
        &mut self,
        _op: &PegInOp,
    ) -> Result<StacksTransaction, PegWalletError> {
        Ok(self.call("mint!".to_string())?)
    }
    fn build_burn_transaction(
        &mut self,
        _op: &PegOutRequestOp,
    ) -> Result<StacksTransaction, PegWalletError> {
        Ok(self.call("burn!".to_string())?)
    }
    fn build_set_address_transaction(
        &mut self,
        _address: PegWalletAddress,
    ) -> Result<StacksTransaction, PegWalletError> {
        Ok(self.call("set-bitcoin-wallet-address".to_string())?)
    }
}
