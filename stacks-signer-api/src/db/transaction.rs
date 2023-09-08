use crate::{
    db::Error,
    transaction::{Transaction, TransactionAddress},
};

use sqlx::SqlitePool;

/// Add a given transaction to the database.
///
/// # Params
/// * pool: SqlitePool - The reference to the SQLite database connection pool.
/// * transaction: Transaction - The transaction object to add to the database.
///
/// # Returns
/// * Result<(), DatabaseError>: The result of the database operation.
pub async fn add_transaction(pool: &SqlitePool, transaction: &Transaction) -> Result<(), Error> {
    let txid = transaction.txid.clone();
    let transaction_kind = &transaction.transaction_kind.to_string();
    let transaction_block_height = transaction
        .transaction_block_height
        .map(|height| height as i64);
    let transaction_deadline_block_height = transaction.transaction_deadline_block_height as i64;
    let transaction_amount = transaction.transaction_amount as i64;
    let transaction_fees = transaction.transaction_fees as i64;
    let memo = &transaction.memo;
    let transaction_originator_address = ""; //&transaction.transaction_originator_address.0;
    let transaction_debit_address = ""; //&transaction.transaction_debit_address.0;
    let transaction_credit_address = ""; //&transaction.transaction_credit_address.0;

    sqlx::query!(
        r#"
    REPLACE INTO transactions (
        txid, transaction_kind, transaction_block_height, transaction_deadline_block_height,
        transaction_amount, transaction_fees, memo, transaction_originator_address,
        transaction_debit_address, transaction_credit_address
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        txid,
        transaction_kind,
        transaction_block_height,
        transaction_deadline_block_height,
        transaction_amount,
        transaction_fees,
        memo,
        transaction_originator_address,
        transaction_debit_address,
        transaction_credit_address
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Get all transactions from the database.
///
/// # Params
/// * pool: SqlitePool - The reference to the SQLite database connection pool.
///
/// # Returns
/// * Result<Vec<Transactions>>: The transactions found in the database.
pub async fn get_transactions(pool: &SqlitePool) -> Result<Vec<Transaction>, Error> {
    sqlx::query!("SELECT * FROM transactions")
        .fetch_all(pool)
        .await?
        .iter()
        .map(|row| {
            let txid = row.txid.clone();
            let transaction_kind = row.transaction_kind.parse()?;
            let transaction_block_height = row.transaction_block_height.map(|height| height as u64);
            let transaction_deadline_block_height = row.transaction_deadline_block_height as u64;
            let transaction_amount = row.transaction_amount as u64;
            let transaction_fees = row.transaction_fees as u64;
            let memo = row.memo.clone();
            let transaction_originator_address =
                TransactionAddress::Bitcoin(row.transaction_originator_address.clone());

            let transaction_debit_address =
                TransactionAddress::Bitcoin(row.transaction_debit_address.clone());

            let transaction_credit_address =
                TransactionAddress::Bitcoin(row.transaction_credit_address.clone());

            Ok(Transaction {
                txid,
                transaction_kind,
                transaction_block_height,
                transaction_deadline_block_height,
                transaction_amount,
                transaction_fees,
                memo,
                transaction_originator_address,
                transaction_debit_address,
                transaction_credit_address,
            })
        })
        .collect()
}

/// Get a transaction with a specific transaction ID from the database.
///
/// # Params
/// * txid: String - The transaction ID to search for.
/// * pool: SqlitePool - The reference to the SQLite database connection pool.
///
/// # Returns
/// * Result<Transaction>: The transaction found in the database.
pub async fn get_transaction_by_id(txid: &str, pool: &SqlitePool) -> Result<Transaction, Error> {
    let row = sqlx::query!("SELECT * FROM transactions WHERE txid = ?", txid)
        .fetch_one(pool)
        .await?;
    let txid = row.txid.clone();
    let transaction_kind = row.transaction_kind.parse()?;
    let transaction_block_height = row.transaction_block_height.map(|height| height as u64);
    let transaction_deadline_block_height = row.transaction_deadline_block_height as u64;
    let transaction_amount = row.transaction_amount as u64;
    let transaction_fees = row.transaction_fees as u64;
    let memo = row.memo.clone();
    let transaction_originator_address =
        TransactionAddress::Bitcoin(row.transaction_originator_address.clone());

    let transaction_debit_address =
        TransactionAddress::Bitcoin(row.transaction_debit_address.clone());

    let transaction_credit_address = TransactionAddress::Bitcoin(row.transaction_credit_address);

    Ok(Transaction {
        txid,
        transaction_kind,
        transaction_block_height,
        transaction_deadline_block_height,
        transaction_amount,
        transaction_fees,
        memo,
        transaction_originator_address,
        transaction_debit_address,
        transaction_credit_address,
    })
}
