use sqlx::SqlitePool;

use crate::{
    db::Error,
    vote::{Vote, VoteTally},
};

/// Add a given vote to the database.
///
/// # Params
/// * pool: SqlitePool - The reference to the SQLite database connection pool.
/// * vote: Vote - The vote object to add to the database.
///
/// # Returns
/// * Result<(), Error>: The result of the database operation.
pub async fn add_vote(vote: &Vote, pool: &SqlitePool) -> Result<(), Error> {
    let txid = vote.txid.clone();
    let vote_status = vote.vote_tally.vote_status.to_string();
    let vote_choice = vote.vote_choice.map(|choice| choice.to_string());
    let vote_mechanism = vote.vote_mechanism.to_string();
    let target_consenus = vote.vote_tally.target_consensus as i64;
    let current_consensus = vote.vote_tally.current_consensus as i64;
    sqlx::query!(
        r#"REPLACE INTO votes (
        txid, vote_status, vote_choice, vote_mechanism, target_consensus, current_consensus
    ) VALUES (?, ?, ?, ?, ?, ?);"#,
        txid,
        vote_status,
        vote_choice,
        vote_mechanism,
        target_consenus,
        current_consensus
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Get a vote with a specific transaction ID from the database.
///
/// # Params
/// * txid: String - The transaction ID to search for.
/// * pool: SqlitePool - The reference to the SQLite database connection pool.
///
/// # Returns
/// * Result<Vote>: The vote found in the database.
pub async fn get_vote_by_id(txid: &str, pool: &SqlitePool) -> Result<Vote, Error> {
    let row = sqlx::query!("SELECT * FROM votes WHERE txid = ?", txid)
        .fetch_one(pool)
        .await?;
    let txid = row.txid.clone();

    let vote_status = row.vote_status.parse()?;
    let vote_choice = row.vote_choice.map(|choice| choice.parse()).transpose()?;

    let vote_mechanism = row.vote_mechanism.parse()?;

    let target_consensus = row.target_consensus as u64;
    let current_consensus = row.current_consensus as u64;

    let vote = Vote {
        txid,
        vote_tally: VoteTally {
            vote_status,
            target_consensus,
            current_consensus,
        },
        vote_choice,
        vote_mechanism,
    };
    Ok(vote)
}
