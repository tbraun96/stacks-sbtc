use std::str::FromStr;

use crate::{
    db::{paginate_items, Error},
    routes::signers::SignerQuery,
    signer::{Signer, Status},
};

use sqlx::SqlitePool;
use warp::http;

/// Add a given signer to the database.
///
/// # Params
/// * signer: Signer - The signer object to add to the database.
/// * pool: SqlitePool - The reference to the SQLite database connection pool.
///
/// # Returns
/// * Result<impl warp::Reply, warp::Rejection>: A Warp reply containing the HTTP response with the result.
pub async fn add_signer(
    signer: Signer,
    pool: SqlitePool,
) -> Result<impl warp::Reply, warp::Rejection> {
    sqlx::query!(
        "INSERT OR REPLACE INTO sbtc_signers (signer_id, user_id, status) VALUES (?1, ?2, ?3)",
        signer.signer_id,
        signer.user_id,
        signer.status
    )
    .execute(&pool)
    .await
    .map_err(Error::from)?;

    Ok(warp::reply::with_status(
        warp::reply::json(&serde_json::json!({ "status": "added" })),
        http::StatusCode::CREATED,
    ))
}

/// Delete a given signer from the database.
///
/// # Params
/// * signer: Signer - The signer object to delete from the database.
/// * pool: SqlitePool - The reference to the SQLite database connection pool.
///
/// # Returns
/// * Result<impl warp::Reply, warp::Rejection>: A Warp reply containing the HTTP response with the result.
pub async fn delete_signer(
    signer: Signer,
    pool: SqlitePool,
) -> Result<impl warp::Reply, warp::Rejection> {
    let rows_deleted = sqlx::query!(
        "DELETE FROM sbtc_signers WHERE signer_id = ?1 AND user_id = ?2",
        signer.signer_id,
        signer.user_id
    )
    .execute(&pool)
    .await
    .map_err(Error::from)?
    .rows_affected();
    // Delete the signer itself
    if rows_deleted == 0 {
        Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({ "error": "not found" })),
            http::StatusCode::NOT_FOUND,
        ))
    } else {
        Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({ "status": "deleted" })),
            http::StatusCode::OK,
        ))
    }
}

/// Get a list of signers from the database given an optional status.
///
/// # Params
/// * query: SignerQuery - A query object containing the optional status filter and pagination options.
/// * pool: SqlitePool - The reference to the SQLite database connection pool.
///
/// # Returns
/// * Result<impl warp::Reply, warp::Rejection>: A Warp reply containing the HTTP response with the list of signers.
pub async fn get_signers(
    query: SignerQuery,
    pool: SqlitePool,
) -> Result<impl warp::Reply, warp::Rejection> {
    if query.page == Some(0) {
        return Err(warp::reject::reject());
    }
    let signers: Vec<(i64, i64, String)> = if let Some(status) = query.status.map(|s| s.as_str()) {
        sqlx::query!(
    "SELECT signer_id, user_id, status FROM sbtc_signers WHERE status = ?1 ORDER BY signer_id ASC", status).fetch_all(&pool).await
        .map_err(Error::from)?
        .iter()
        .map(|row| (row.signer_id, row.user_id, row.status.clone()))
        .collect()
    } else {
        sqlx::query!("SELECT signer_id, user_id, status FROM sbtc_signers ORDER BY signer_id ASC")
            .fetch_all(&pool)
            .await
            .map_err(Error::from)?
            .iter()
            .map(|row| (row.signer_id, row.user_id, row.status.clone()))
            .collect()
    };
    let displayed_signers = paginate_items(&signers, query.page, query.limit);

    Ok(warp::reply::with_status(
        warp::reply::json(&serde_json::json!(displayed_signers)),
        http::StatusCode::OK,
    ))
}

// Private Util functions

#[allow(dead_code)]
async fn get_number_of_signers(signer: Signer, pool: SqlitePool) -> usize {
    sqlx::query!(
        "SELECT * FROM sbtc_signers WHERE signer_id = ?1 AND user_id = ?2",
        signer.signer_id,
        signer.user_id
    )
    .fetch_all(&pool)
    .await
    .expect("Failed to get row count")
    .len()
}

#[allow(dead_code)]
async fn get_first_signer(pool: SqlitePool) -> Signer {
    let row =
        sqlx::query!("SELECT signer_id, user_id, status FROM sbtc_signers ORDER BY signer_id ASC")
            .fetch_one(&pool)
            .await
            .expect("Failed to get added signer");

    Signer {
        signer_id: row.signer_id,
        user_id: row.user_id,
        status: Status::from_str(&row.status).unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::init_pool, signer::Status};
    use warp::http::StatusCode;
    use warp::Reply;

    async fn init_db() -> SqlitePool {
        init_pool(None)
            .await
            .expect("Failed to initialize a new database pool.")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ntest::timeout(1000)]
    async fn test_add_signer() {
        let pool = init_db().await;
        let expected_signer = Signer {
            signer_id: 1,
            user_id: 1,
            status: Status::Active,
        };

        let response = add_signer(expected_signer, pool.clone())
            .await
            .expect("failed to add signer");
        assert_eq!(response.into_response().status(), StatusCode::CREATED);

        assert_eq!(expected_signer, get_first_signer(pool).await);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ntest::timeout(1000)]
    async fn test_delete_signer() {
        let pool = init_db().await;
        let signer = Signer {
            signer_id: 1,
            user_id: 1,
            status: Status::Active,
        };

        add_signer(signer, pool.clone())
            .await
            .expect("failed to add signer");

        let response = delete_signer(signer, pool.clone())
            .await
            .expect("failed to delete signer");
        assert_eq!(response.into_response().status(), StatusCode::OK);

        assert_eq!(get_number_of_signers(signer, pool).await, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ntest::timeout(1000)]
    async fn test_get_signers() {
        let pool = init_db().await;
        let signers_to_insert = vec![
            Signer {
                signer_id: 1,
                user_id: 1,
                status: Status::Active,
            },
            Signer {
                signer_id: 2,
                user_id: 2,
                status: Status::Active,
            },
            Signer {
                signer_id: 3,
                user_id: 3,
                status: Status::Inactive,
            },
        ];

        for signer in signers_to_insert.clone() {
            add_signer(signer, pool.clone())
                .await
                .expect("failed to add signer");
        }

        let query = SignerQuery {
            page: Some(1),
            limit: Some(2),
            status: Some(Status::Active),
        };
        let body = get_signers(query.clone(), pool.clone())
            .await
            .expect("failed to get signers")
            .into_response()
            .into_body();
        let body_bytes = warp::hyper::body::to_bytes(body)
            .await
            .expect("failed to get response bytes");

        let signers: Vec<Signer> =
            serde_json::from_slice(&body_bytes).expect("failed to deserialize signers");
        assert_eq!(signers, &signers_to_insert[0..2]);

        let query = SignerQuery {
            page: Some(2),
            limit: Some(2),
            status: None,
        };

        let body = get_signers(query.clone(), pool.clone())
            .await
            .expect("failed to get signers")
            .into_response()
            .into_body();
        let body_bytes = warp::hyper::body::to_bytes(body)
            .await
            .expect("failed to get response bytes");

        let signers: Vec<Signer> =
            serde_json::from_slice(&body_bytes).expect("failed to deserialize signers");
        assert_eq!(signers, &signers_to_insert[2..3]);

        let query = SignerQuery {
            page: None,
            limit: None,
            status: None,
        };

        let body = get_signers(query.clone(), pool.clone())
            .await
            .expect("failed to get signers")
            .into_response()
            .into_body();
        let body_bytes = warp::hyper::body::to_bytes(body)
            .await
            .expect("failed to get response bytes");

        let signers: Vec<Signer> =
            serde_json::from_slice(&body_bytes).expect("failed to deserialize signers");
        assert_eq!(signers, signers_to_insert);
    }
}
