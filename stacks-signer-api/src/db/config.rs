use crate::{config::Config, db::Error};

use secp256k1::{PublicKey, SecretKey};
use sqlx::SqlitePool;
use std::str::FromStr;

/// Helper function for retriving a signer from the database given a signer ID.
pub async fn get_config(pool: &SqlitePool) -> Result<Config, Error> {
    let row = sqlx::query!("SELECT * FROM config").fetch_one(pool).await?;
    let secret_bytes = hex::decode(row.secret_key)?;
    Ok(Config {
        secret_key: SecretKey::from_slice(&secret_bytes)?,
        auto_approve_max_amount: row.auto_approve_max_amount as u64,
        delegate_public_key: PublicKey::from_str(row.delegate_public_key.as_str())?,
        delegator_public_keys: get_delegator_public_keys(pool).await?,
        auto_deny_addresses: get_auto_deny_addresses(pool).await?,
    })
}

/// Helper function for adding a signer to the database.
pub async fn update_config(pool: &SqlitePool, config: &Config) -> Result<(), Error> {
    let secret_key = hex::encode(config.secret_key.secret_bytes()).to_string();
    let auto_approve_max_amount = config.auto_approve_max_amount as i64;
    let delegate_public_key = config.delegate_public_key.to_string();
    sqlx::query!(
        "REPLACE INTO config (id, secret_key, delegate_public_key, auto_approve_max_amount) VALUES (?1, ?2, ?3, ?4)",
        1,
        secret_key,
        delegate_public_key,
        auto_approve_max_amount,
    )
    .execute(pool)
    .await?;
    for delegator_public_key in &config.delegator_public_keys {
        let delegator_public_key = delegator_public_key.to_string();
        sqlx::query!(
            "REPLACE INTO delegator_public_keys (public_key) VALUES (?1)",
            delegator_public_key,
        )
        .execute(pool)
        .await?;
    }
    for address in &config.auto_deny_addresses {
        sqlx::query!(
            "REPLACE INTO auto_deny_addresses (address) VALUES (?1)",
            address,
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Helper function for retrieving the auto deny addresses for a signer.
async fn get_auto_deny_addresses(pool: &SqlitePool) -> Result<Vec<String>, Error> {
    let addresses: Vec<String> = sqlx::query!("SELECT address FROM auto_deny_addresses")
        .fetch_all(pool)
        .await?
        .iter()
        .map(|row| row.address.clone())
        .collect();
    Ok(addresses)
}

/// Helper function for retrieving the delegator public keys for a signer.
async fn get_delegator_public_keys(pool: &SqlitePool) -> Result<Vec<PublicKey>, Error> {
    sqlx::query!("SELECT public_key FROM delegator_public_keys")
        .fetch_all(pool)
        .await?
        .iter()
        .map(|row| PublicKey::from_str(row.public_key.as_str()).map_err(Error::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, db::init_pool};

    //const TEST_PUBLIC_KEY_1: &str =
    //    "025972a1f2532b44348501075075b31eb21c02eef276b91db99d30703f2081b773";
    const TEST_SECRET_KEY_1: &str =
        "26F85CE8B2C635AD92F6148E4443FE415F512F3F29F44AB0E2CBDA819295BBD5";
    const TEST_PUBLIC_KEY_2: &str =
        "039d3a5ea41730c84e3dd3b513a0a8349b2ed7d178fb026b7b771cea6c395b7870";
    const TEST_PUBLIC_KEY_3: &str =
        "0355f69447d2fb4212c20360c67506656c39578ce1ccb0b2e5c1976edd5a51ea4d";

    async fn init_db() -> SqlitePool {
        init_pool(None)
            .await
            .expect("Failed to initialize a new database pool.")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ntest::timeout(1000)]
    async fn test_update_config() {
        let pool = init_db().await;
        let mut expected_config =
            Config::from_secret_key(TEST_SECRET_KEY_1).expect("Failed to create config.");
        // Insert an initial config:
        update_config(&pool, &expected_config)
            .await
            .expect("failed to add config");
        let config = get_config(&pool).await.expect("Failed to get signers");
        assert_eq!(
            config.auto_approve_max_amount,
            expected_config.auto_approve_max_amount
        );
        // Update the config and verify the database updated
        expected_config.auto_approve_max_amount = 10;
        update_config(&pool, &expected_config)
            .await
            .expect("failed to add config");

        let config = get_config(&pool).await.expect("Failed to get signers");
        assert_eq!(
            config.auto_approve_max_amount,
            expected_config.auto_approve_max_amount
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ntest::timeout(1000)]
    async fn test_get_config() {
        let pool = init_db().await;
        // When the database is empty, it should fail to retrieve a config
        assert!(get_config(&pool).await.is_err());

        let expected_config =
            Config::from_secret_key(TEST_SECRET_KEY_1).expect("Failed to create config.");
        // Insert an initial config:
        update_config(&pool, &expected_config)
            .await
            .expect("failed to add config");
        let config = get_config(&pool).await.expect("Failed to get signers");

        assert_eq!(config.secret_key, expected_config.secret_key);
        assert_eq!(
            config.auto_approve_max_amount,
            expected_config.auto_approve_max_amount
        );
        assert_eq!(
            config.delegate_public_key,
            expected_config.delegate_public_key
        );
        assert_eq!(
            config.delegator_public_keys,
            expected_config.delegator_public_keys
        );
        assert_eq!(
            config.auto_deny_addresses,
            expected_config.auto_deny_addresses
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ntest::timeout(1000)]
    async fn test_get_delegator_public_keys() {
        let pool = init_db().await;
        let mut config =
            Config::from_secret_key(TEST_SECRET_KEY_1).expect("Failed to create config.");
        config.delegator_public_keys = vec![
            PublicKey::from_str(TEST_PUBLIC_KEY_2).expect("Failed to parse public key."),
            PublicKey::from_str(TEST_PUBLIC_KEY_3).expect("Failed to parse public key."),
        ];

        update_config(&pool, &config)
            .await
            .expect("failed to add config");
        let keys = get_delegator_public_keys(&pool)
            .await
            .expect("failed to get delegator public keys");
        assert_eq!(keys, config.delegator_public_keys);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ntest::timeout(1000)]
    async fn test_get_auto_deny_addresses() {
        let pool = init_db().await;
        let mut config =
            Config::from_secret_key(TEST_SECRET_KEY_1).expect("Failed to create config.");
        config.auto_deny_addresses = vec![
            "address1".to_string(),
            "address2".to_string(),
            "address3".to_string(),
        ];

        update_config(&pool, &config)
            .await
            .expect("failed to add config");
        let addresses = get_auto_deny_addresses(&pool)
            .await
            .expect("failed to get auto deny addresses");
        assert_eq!(addresses, config.auto_deny_addresses);
    }
}
