use secp256k1::{PublicKey, Secp256k1, SecretKey};
use serde::{Deserialize, Serialize};
use utoipa::{ToResponse, ToSchema};

const DEFAULT_MAX_AMOUNT: u64 = 100_000;

/// Custom error type for this database module
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Config error due to an invalid secret key
    #[error("Hex Error: {0}")]
    HexError(#[from] hex::FromHexError),
    /// Config error due to secp256k1 internal error
    #[error("Secp256k1 Error: {0}")]
    Secp256k1Error(#[from] secp256k1::Error),
    /// Config error due to an IO error
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),
    /// Config error due to toml deserialization error
    #[error("Toml Error: {0}")]
    TomlError(#[from] toml::de::Error),
}

#[derive(serde::Deserialize)]
/// A raw signer configuration that can be deserialized from a TOML file.
pub struct RawConfig {
    /// The signer's secret key.
    pub secret_key: SecretKey,
    /// The maximum dollar amount of a transaction that will be auto approved
    pub delegate_public_key: Option<PublicKey>,
    /// The public keys of signers that this signer has agreed to sign on behalf of
    pub delegator_public_keys: Option<Vec<PublicKey>>,
    /// The addresses to be auto denied
    pub auto_deny_addresses: Option<Vec<String>>,
    /// The maximum dollar amount of a transaction that will be auto approved
    pub auto_approve_max_amount: Option<u64>,
}

impl RawConfig {
    /// Try to create a raw configuration from a given path to a TOML file.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        let raw_config: RawConfig = toml::from_str(&std::fs::read_to_string(path)?)?;
        Ok(raw_config)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Deserialize, Serialize, ToResponse, ToSchema)]
/// A signer configuration.
pub struct Config {
    /// The signer's secret key.
    #[schema(value_type = String)]
    pub secret_key: SecretKey,
    /// The maximum dollar amount of a transaction that will be auto approved
    pub auto_approve_max_amount: u64,
    /// The public key of the signer being delegated to
    #[schema(value_type = String)]
    pub delegate_public_key: PublicKey,
    /// The public keys of signers that this signer has agreed to sign on behalf of
    #[schema(value_type = Vec<String>)]
    pub delegator_public_keys: Vec<PublicKey>,
    /// The addresses to be auto denied
    pub auto_deny_addresses: Vec<String>,
}

impl Config {
    /// Create a new signer configuration with a given secret key.
    pub fn new(secret_key: SecretKey) -> Self {
        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &secret_key);
        Self {
            secret_key,
            delegate_public_key: public_key,
            auto_approve_max_amount: DEFAULT_MAX_AMOUNT,
            delegator_public_keys: vec![],
            auto_deny_addresses: vec![],
        }
    }

    /// Try to create a new signer configuration with a given hex encoded secret key string.
    pub fn from_secret_key(secret_key: &str) -> Result<Self, Error> {
        let secret_bytes = hex::decode(secret_key)?;
        let secret_key = SecretKey::from_slice(&secret_bytes)?;
        Ok(Self::new(secret_key))
    }

    /// Try to create a configuration from a given path to a TOML file.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self, Error> {
        let raw_config = RawConfig::from_path(path)?;
        Config::try_from(raw_config)
    }
}

impl TryFrom<RawConfig> for Config {
    type Error = Error;
    fn try_from(raw_config: RawConfig) -> Result<Config, Error> {
        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &raw_config.secret_key);
        Ok(Config {
            secret_key: raw_config.secret_key,
            delegate_public_key: raw_config.delegate_public_key.unwrap_or(public_key),
            auto_approve_max_amount: raw_config
                .auto_approve_max_amount
                .unwrap_or(DEFAULT_MAX_AMOUNT),
            delegator_public_keys: raw_config.delegator_public_keys.unwrap_or(vec![]),
            auto_deny_addresses: raw_config.auto_deny_addresses.unwrap_or(vec![]),
        })
    }
}
