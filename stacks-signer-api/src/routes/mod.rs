#![deny(missing_docs)]
/// Config Routes
pub mod config;
/// Transaction Routes
pub mod transactions;
/// Vote Routes
pub mod vote;

use serde::{de::DeserializeOwned, Deserialize};
use sqlx::SqlitePool;
use std::convert::Infallible;
use warp::{Filter, Rejection, Reply};

use self::{
    config::{get_config_route, update_config_route},
    transactions::{get_transaction_by_id_route, get_transactions_route},
    vote::vote_route,
};

#[derive(Debug, Deserialize, Clone)]
/// The query parameters for get routes that return a vector of items.
pub struct Pagination {
    /// The page number.
    pub page: Option<usize>,
    /// The limit of items per page.
    pub limit: Option<usize>,
}

/// Paginate a slice of items.
///
/// This utility function slices a given set of items based on the specified `page` and `limit`.
/// If `page` and/or `limit` are not provided (None), the function will use default values.
///
/// # Params
/// * items: &[T] - The reference to the slice of items to be paginated.
/// * page: Option<usize> - The optional page number for pagination (1-based index).
/// * limit: Option<usize> - The optional limit representing the maximum number of items per page.
///
/// # Returns
/// * &[T]: A slice of the original items, paginated according to the provided page and limit.
pub fn paginate_items<T>(items: &[T], page: Option<usize>, limit: Option<usize>) -> &[T] {
    let page = page.unwrap_or(1);
    let limit = limit.unwrap_or(usize::MAX);

    let start_index = items.len().min((page - 1) * limit);
    let end_index = items.len().min(start_index + limit);
    &items[start_index.min(end_index)..end_index]
}

/// A helper function to extract a JSON body from a request.
pub fn json_body<T: std::marker::Send + DeserializeOwned>(
) -> impl Filter<Extract = (T,), Error = warp::Rejection> + Clone {
    // When accepting a body, we want a JSON body
    // (and to reject huge payloads)...
    warp::body::content_length_limit(1024 * 16).and(warp::body::json::<T>())
}

/// A helper function to extract a database pool from a request.
pub fn with_pool(
    pool: SqlitePool,
) -> impl Filter<Extract = (SqlitePool,), Error = Infallible> + Clone {
    warp::any().map(move || pool.clone())
}

/// A helper function to combine all routes into one Warp Filter
///
/// # Params
/// * pool: SqlitePool - The reference to the Sqlite database connection pool.
///
/// # Returns
/// * impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone: The Warp filter for the routes.
pub fn all_routes(
    pool: SqlitePool,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    // Set up the routes
    // Config routes
    let update_config_route = update_config_route(pool.clone());
    let get_config_route = get_config_route(pool.clone());
    // Transaction routes
    let get_transactions_route = get_transactions_route(pool.clone());
    let get_transaction_by_id_route = get_transaction_by_id_route(pool.clone());
    // Vote routes
    let vote_route = vote_route(pool);

    // Combine and return the routes in a single filter
    update_config_route
        .or(get_config_route)
        .or(get_transactions_route)
        .or(get_transaction_by_id_route)
        .or(vote_route)
}
