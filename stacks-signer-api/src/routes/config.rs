use std::convert::Infallible;

use crate::{
    config::Config,
    db,
    routes::{json_body, with_pool},
};
use sqlx::SqlitePool;
use warp::{hyper::StatusCode, Filter, Reply};

/// Route for updating the signer config.
///
/// # Params
/// * pool: SqlitePool - The reference to the Sqlite database connection pool.
///
/// # Returns
/// * impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone:
///   The Warp filter for the update_config_route endpoint for routing HTTP requests.
pub fn update_config_route(
    pool: SqlitePool,
) -> impl Filter<Extract = (impl Reply,), Error = warp::Rejection> + Clone {
    warp::post()
        .and(warp::path!("v1" / "config"))
        .and(warp::path::end())
        .and(json_body::<Config>())
        .and(with_pool(pool))
        .and_then(update_config)
}

/// Route for fetching the signer's configuration.
///
/// # Params
/// * pool: SqlitePool - The reference to the Sqlite database connection pool.
///
/// # Returns
/// * impl Filter<Extract = (impl warp::Reply,), Error = warp::Rejection> + Clone:
///  The Warp filter for the get_config_route endpoint for routing HTTP requests.
pub fn get_config_route(
    pool: SqlitePool,
) -> impl Filter<Extract = (impl Reply,), Error = warp::Rejection> + Clone {
    warp::get()
        .and(warp::path!("v1" / "config"))
        .and(warp::path::end())
        .and(with_pool(pool))
        .and_then(get_config)
}

/// Update the signer's configuration.
#[utoipa::path(
    post,
    path = "/v1/config",
    request_body = Config,
    responses(
        (status = OK, description = "Config updated successfully."),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error occurred.", body = ErrorResponse)
    ),
)]
pub async fn update_config(config: Config, pool: SqlitePool) -> Result<Box<dyn Reply>, Infallible> {
    if db::config::update_config(&pool, &config).await.is_ok() {
        Ok(Box::new(StatusCode::OK))
    } else {
        Ok(Box::new(StatusCode::NOT_FOUND))
    }
}

/// Get the signer's configuration.
#[utoipa::path(
    get,
    path = "/v1/config",
    responses(
        (status = OK, description = "Config retrieved successfully.", body = Config),
        (status = INTERNAL_SERVER_ERROR, description = "Internal server error occurred.", body = ErrorResponse)
    ),
)]
pub async fn get_config(pool: SqlitePool) -> Result<Box<dyn Reply>, Infallible> {
    if let Ok(config) = db::config::get_config(&pool).await {
        Ok(Box::new(warp::reply::with_status(
            warp::reply::json(&config),
            StatusCode::OK,
        )))
    } else {
        Ok(Box::new(StatusCode::NOT_FOUND))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db::init_pool;
    use secp256k1::PublicKey;
    use std::str::FromStr;

    const TEST_PUBLIC_KEY_1: &str =
        "025972a1f2532b44348501075075b31eb21c02eef276b91db99d30703f2081b773";
    const TEST_SECRET_KEY_1: &str =
        "26F85CE8B2C635AD92F6148E4443FE415F512F3F29F44AB0E2CBDA819295BBD5";
    const TEST_PUBLIC_KEY_2: &str =
        "039d3a5ea41730c84e3dd3b513a0a8349b2ed7d178fb026b7b771cea6c395b7870";

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
            Config::from_secret_key(TEST_SECRET_KEY_1).expect("Failed to create config");
        // First add a config.
        db::config::update_config(&pool, &expected_config)
            .await
            .expect("Failed to add configuration to database.");

        // Update the config and use the API to update the database
        expected_config.auto_approve_max_amount = 10;
        expected_config.delegator_public_keys = vec![
            PublicKey::from_str(TEST_PUBLIC_KEY_1).expect("Failed to create public key"),
            PublicKey::from_str(TEST_PUBLIC_KEY_2).expect("Failed to create public key"),
        ];
        expected_config.auto_deny_addresses = vec!["Address1".to_string(), "Address2".to_string()];

        let api = warp::test::request()
            .path("/v1/config")
            .method("POST")
            .json(&expected_config)
            .reply(&update_config_route(pool.clone()))
            .await;

        assert_eq!(api.status(), StatusCode::OK);
        assert!(api.body().is_empty());

        let config = db::config::get_config(&pool)
            .await
            .expect("Failed to get configuration from the database");
        assert_eq!(config.secret_key, expected_config.secret_key);
        assert_eq!(
            config.delegate_public_key,
            expected_config.delegate_public_key
        );
        assert_eq!(
            config.auto_approve_max_amount,
            expected_config.auto_approve_max_amount
        );
        assert_eq!(
            config.auto_deny_addresses,
            expected_config.auto_deny_addresses
        );
        assert_eq!(
            config.delegator_public_keys,
            expected_config.delegator_public_keys
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ntest::timeout(1000)]
    async fn test_get_config() {
        let pool = init_db().await;
        let expected_config =
            Config::from_secret_key(TEST_SECRET_KEY_1).expect("Failed to create config");
        db::config::update_config(&pool, &expected_config)
            .await
            .expect("Failed to add configuration to database.");

        let api = warp::test::request()
            .path("/v1/config")
            .method("GET")
            .header("content-type", "application/json")
            .reply(&get_config_route(pool))
            .await;
        let body = api.body();
        let config: Config = serde_json::from_slice(body).expect("failed to deserialize config");

        assert_eq!(config.secret_key, expected_config.secret_key);
        assert_eq!(
            config.delegate_public_key,
            expected_config.delegate_public_key
        );
        assert_eq!(
            config.auto_approve_max_amount,
            expected_config.auto_approve_max_amount
        );
        assert_eq!(
            config.auto_deny_addresses,
            expected_config.auto_deny_addresses
        );
        assert_eq!(
            config.delegator_public_keys,
            expected_config.delegator_public_keys
        );
    }
}
