#![deny(missing_docs)]
/// Module that handles signers-related database operations
pub mod config;
/// Module that handles transaction-related database operations
pub mod transaction;
/// Module that handles vote-related database operations
pub mod vote;

use parse_display::ParseError;
use sqlx::SqlitePool;

/// Custom error type for this database module
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// Sqlx related error
    #[error("Sqlx Error: {0}")]
    SqlxError(#[from] sqlx::Error),
    /// Parse related error
    #[error("Parsing error occurred due to malformed data")]
    MalformedData(#[from] ParseError),
    /// Secp256k1 related error
    #[error("Secp256k1 Error: {0}")]
    Secp256k1Error(#[from] secp256k1::Error),
    /// Sqlx Migration Error
    #[error("Sqlx Migration Error: {0}")]
    SqlxMigrationError(#[from] sqlx::migrate::MigrateError),
    /// Secret key parsing related error
    #[error("Invalid Secret Key: {0}")]
    InvalidSecretKey(#[from] hex::FromHexError),
}

impl warp::reject::Reject for Error {}

/// Initialize the database pool from the given file path or in memory if none is provided.
///
/// # Params
/// * path: Option<&str> - Optional file path to the SQLite database, or None to use in-memory storage.
///
/// # Returns
/// * Result<SqlitePool, Error>: Result containing the initialized SqlitePool, or an Error if initialization failed.
pub async fn init_pool(path: Option<String>) -> Result<SqlitePool, Error> {
    let pool = match path {
        Some(path) => SqlitePool::connect(&path).await?,
        None => SqlitePool::connect("sqlite::memory:").await?,
    };
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}
