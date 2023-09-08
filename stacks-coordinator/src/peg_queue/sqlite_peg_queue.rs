use async_trait::async_trait;
use sqlx::{Connection, Error as SqlxError, FromRow, Row, SqliteConnection as SqlxConnection};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use blockstack_lib::burnchains::Txid;
use blockstack_lib::types::chainstate::BurnchainHeaderHash;
use blockstack_lib::util::HexError;
use sqlx::sqlite::SqliteRow;
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::peg_queue::{Error as PegQueueError, PegQueue, SbtcOp};
use crate::stacks_node::{Error as StacksNodeError, PegInOp, PegOutRequestOp, StacksNode};

use tracing::{debug, info};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Sqlx Error: {0}")]
    SqlxError(#[from] SqlxError),
    #[error("JSON serialization failure: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Hex codec error: {0}")]
    HexError(#[from] HexError),
    #[error("Did not recognize status: {0}")]
    InvalidStatusError(String),
}

// Workaround to allow non-perfect conversions in `Entry::from_row`
impl From<Error> for sqlx::Error {
    fn from(err: Error) -> Self {
        Self::ColumnNotFound(err.to_string())
    }
}

pub struct SqlitePegQueue {
    conn: Arc<Mutex<SqlxConnection>>,
}

impl SqlitePegQueue {
    pub async fn new<P: AsRef<Path>>(
        path: P,
        start_block_height: Option<u64>,
        current_block_height: u64,
    ) -> Result<Self, Error> {
        Self::from_connection(
            SqlxConnection::connect(&format!("{}", path.as_ref().display())).await?,
            start_block_height,
            current_block_height,
        )
        .await
    }

    pub async fn in_memory(
        start_block_height: Option<u64>,
        current_block_height: u64,
    ) -> Result<Self, Error> {
        Self::from_connection(
            SqlxConnection::connect("sqlite::memory:").await?,
            start_block_height,
            current_block_height,
        )
        .await
    }

    async fn from_connection(
        mut conn: SqlxConnection,
        start_block_height: Option<u64>,
        current_block_height: u64,
    ) -> Result<Self, Error> {
        sqlx::query(Self::create_sbtc_ops_table())
            .execute(&mut conn)
            .await?;

        sqlx::query(Self::create_metadata_table())
            .execute(&mut conn)
            .await?;

        let this = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        // Prevent overflow by calling saturating sub to ensure we don't go below 0
        if let Some(start_block_height) = start_block_height {
            this.insert_last_processed_block_height(start_block_height.saturating_sub(1))
                .await?;
        } else if this.last_processed_block_height().await.is_err() {
            // If we don't have a last processed block height, set it to the current block height
            this.insert_last_processed_block_height(current_block_height.saturating_sub(1))
                .await?;
        }
        Ok(this)
    }

    async fn poll_peg_in_ops<N: StacksNode>(
        &self,
        stacks_node: &N,
        block_height: u64,
    ) -> Result<(), PegQueueError> {
        match stacks_node.get_peg_in_ops(block_height).await {
            Err(StacksNodeError::UnknownBlockHeight(height)) => {
                debug!("Failed to find burn block height {}", height);
            }
            Err(e) => return Err(PegQueueError::from(e)),
            Ok(peg_in_ops) => {
                for peg_in_op in peg_in_ops {
                    let entry = Entry::from(peg_in_op);
                    self.insert(&entry).await?;
                }
            }
        }
        Ok(())
    }

    async fn poll_peg_out_request_ops<N: StacksNode>(
        &self,
        stacks_node: &N,
        block_height: u64,
    ) -> Result<(), PegQueueError> {
        match stacks_node.get_peg_out_request_ops(block_height).await {
            Err(StacksNodeError::UnknownBlockHeight(height)) => {
                debug!("Failed to find burn block height {}", height);
            }
            Err(e) => return Err(PegQueueError::from(e)),
            Ok(peg_out_request_ops) => {
                for peg_out_request_op in peg_out_request_ops {
                    let entry = Entry::from(peg_out_request_op);
                    self.insert(&entry).await?;
                }
            }
        }
        Ok(())
    }

    async fn insert(&self, entry: &Entry) -> Result<(), Error> {
        let mut conn = self.get_conn().await;
        sqlx::query(Self::sql_insert())
            .bind(entry.txid.to_hex())
            .bind(entry.burn_header_hash.to_hex())
            .bind(entry.block_height as i64)
            .bind(serde_json::to_string(&entry.op)?)
            .bind(entry.status.as_str())
            .execute(&mut *conn)
            .await?;

        Ok(())
    }

    async fn get_single_entry_with_status(&self, status: &Status) -> Result<Option<Entry>, Error> {
        let mut conn = self.get_conn().await;
        let result = sqlx::query_as::<_, Entry>(Self::sql_select_status())
            .bind(status.as_str())
            .fetch_optional(&mut *conn)
            .await?;
        Ok(result)
    }

    async fn get_entry(
        &self,
        txid: &Txid,
        burn_header_hash: &BurnchainHeaderHash,
    ) -> Result<Entry, Error> {
        let mut conn = self.get_conn().await;
        let result = sqlx::query_as::<_, Entry>(Self::sql_select_pk())
            .bind(txid.to_hex())
            .bind(burn_header_hash.to_hex())
            .fetch_one(&mut *conn)
            .await?;
        Ok(result)
    }

    async fn last_processed_block_height(&self) -> Result<u64, Error> {
        let mut conn = self.get_conn().await;
        let height: Option<i64> =
            sqlx::query_scalar(Self::sql_select_last_processed_block_height())
                .fetch_optional(&mut *conn)
                .await?;

        height
            .map(|height| height as u64)
            .ok_or(Error::SqlxError(SqlxError::RowNotFound))
    }

    async fn insert_last_processed_block_height(&self, height: u64) -> Result<(), Error> {
        let mut conn = self.get_conn().await;
        sqlx::query(Self::sql_insert_last_processed_block_height())
            .bind(height as i64)
            .execute(&mut *conn)
            .await?;

        Ok(())
    }

    async fn get_conn(&self) -> OwnedMutexGuard<SqlxConnection> {
        self.conn.clone().lock_owned().await
    }

    const fn create_sbtc_ops_table() -> &'static str {
        r#"
        CREATE TABLE IF NOT EXISTS sbtc_ops (
            txid TEXT NOT NULL,
            burn_header_hash TEXT NOT NULL,
            block_height INTEGER NOT NULL,
            op TEXT NOT NULL,
            status TEXT NOT NULL,

            PRIMARY KEY(txid, burn_header_hash)
        )
        "#
    }

    const fn create_metadata_table() -> &'static str {
        r#"
        CREATE TABLE IF NOT EXISTS peg_queue_metadata (
            id TEXT NOT NULL,

            last_processed_block_height INTEGER NOT NULL,

            PRIMARY KEY(id)
        )
        "#
    }

    const fn sql_insert() -> &'static str {
        r#"
        REPLACE INTO sbtc_ops (txid, burn_header_hash, block_height, op, status) VALUES (?, ?, ?, ?, ?)
        "#
    }

    const fn sql_select_status() -> &'static str {
        r#"
        SELECT txid, burn_header_hash, block_height, op, status FROM sbtc_ops WHERE status=? ORDER BY block_height, op ASC
        "#
    }

    const fn sql_select_pk() -> &'static str {
        r#"
        SELECT txid, burn_header_hash, block_height, op, status FROM sbtc_ops WHERE txid=? AND burn_header_hash=?
        "#
    }

    const fn sql_select_last_processed_block_height() -> &'static str {
        r#"
            SELECT last_processed_block_height FROM peg_queue_metadata WHERE id='peg_queue'
        "#
    }

    const fn sql_insert_last_processed_block_height() -> &'static str {
        r#"
            REPLACE INTO peg_queue_metadata (id, last_processed_block_height) VALUES ('peg_queue', ?)
        "#
    }
}

#[async_trait]
impl PegQueue for SqlitePegQueue {
    async fn sbtc_op(&self) -> Result<Option<SbtcOp>, PegQueueError> {
        let maybe_entry = self.get_single_entry_with_status(&Status::New).await?;

        let Some(mut entry) = maybe_entry else {
            return Ok(None);
        };

        entry.status = Status::Pending;
        self.insert(&entry).await?;

        Ok(Some(entry.op))
    }

    async fn poll<N: StacksNode>(&self, stacks_node: &N) -> Result<(), PegQueueError> {
        let target_block_height = stacks_node.burn_block_height().await?;
        let start_block_height = self
            .last_processed_block_height()
            .await
            .map(|count| count + 1)?;

        if start_block_height > target_block_height {
            info!("No new blocks to process");
            return Ok(());
        }

        info!(
            "Checking block heights {} to {}",
            start_block_height, target_block_height
        );

        for block_height in start_block_height..=target_block_height {
            self.poll_peg_in_ops(stacks_node, block_height).await?;
            self.poll_peg_out_request_ops(stacks_node, block_height)
                .await?;
            self.insert_last_processed_block_height(block_height)
                .await?;
            info!("Processed block height {}", block_height);
        }
        Ok(())
    }

    async fn acknowledge(
        &self,
        txid: &Txid,
        burn_header_hash: &BurnchainHeaderHash,
    ) -> Result<(), PegQueueError> {
        let mut entry = self.get_entry(txid, burn_header_hash).await?;

        entry.status = Status::Acknowledged;
        self.insert(&entry).await?;

        Ok(())
    }
}

#[derive(Debug)]
struct Entry {
    burn_header_hash: BurnchainHeaderHash,
    txid: Txid,
    block_height: u64,
    op: SbtcOp,
    status: Status,
}

impl<'r> FromRow<'r, SqliteRow> for Entry {
    fn from_row(row: &'r SqliteRow) -> Result<Self, SqlxError> {
        let txid = Txid::from_hex(&row.try_get::<String, _>(0)?).map_err(Error::from)?;

        let burn_header_hash =
            BurnchainHeaderHash::from_hex(&row.try_get::<String, _>(1)?).map_err(Error::from)?;

        let block_height = row.try_get::<i64, _>(2)? as u64; // Stacks will crash before the coordinator if this is invalid

        let op: SbtcOp =
            serde_json::from_str(&row.try_get::<String, _>(3)?).map_err(Error::from)?;

        let status: Status = row.try_get::<String, _>(4)?.parse()?;

        Ok(Self {
            burn_header_hash,
            txid,
            block_height,
            op,
            status,
        })
    }
}

impl From<PegInOp> for Entry {
    fn from(op: PegInOp) -> Self {
        Self {
            block_height: op.block_height,
            status: Status::New,
            txid: op.txid,
            burn_header_hash: op.burn_header_hash,
            op: SbtcOp::PegIn(op),
        }
    }
}

impl From<PegOutRequestOp> for Entry {
    fn from(op: PegOutRequestOp) -> Self {
        Self {
            block_height: op.block_height,
            status: Status::New,
            txid: op.txid,
            burn_header_hash: op.burn_header_hash,
            op: SbtcOp::PegOutRequest(op),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Status {
    New,
    Pending,
    Acknowledged,
}

impl Status {
    fn as_str(&self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Pending => "pending",
            Self::Acknowledged => "acknowledged",
        }
    }
}

impl FromStr for Status {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        Ok(match s {
            "new" => Self::New,
            "pending" => Self::Pending,
            "acknowledged" => Self::Acknowledged,
            other => return Err(Error::InvalidStatusError(other.to_owned())),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::stacks_node;

    use blockstack_lib::{
        chainstate::stacks::address::PoxAddress,
        types::chainstate::StacksAddress,
        util::{hash::Hash160, secp256k1::MessageSignature},
    };
    use std::{collections::hash_map::DefaultHasher, hash::Hasher};

    use crate::peg_queue::PegQueue;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn calling_sbtc_op_should_return_new_peg_ops() {
        let peg_queue = SqlitePegQueue::in_memory(Some(1), 2).await.unwrap();
        let number_of_simulated_blocks: u64 = 3;

        let stacks_node_mock = default_stacks_node_mock(number_of_simulated_blocks);

        // No ops before polling
        assert!(peg_queue.sbtc_op().await.unwrap().is_none());

        // Should cause the peg_queue to fetch 3 peg in ops
        peg_queue.poll(&stacks_node_mock).await.unwrap();

        for height in 1..=number_of_simulated_blocks {
            let next_op = peg_queue.sbtc_op().await.unwrap().unwrap();
            assert!(next_op.as_peg_in().is_some());
            assert_eq!(next_op.as_peg_in().unwrap().block_height, height);

            let next_op = peg_queue.sbtc_op().await.unwrap().unwrap();
            assert!(next_op.as_peg_out_request().is_some());
            assert_eq!(next_op.as_peg_out_request().unwrap().block_height, height);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn calling_poll_should_not_query_new_ops_if_at_block_height() {
        let peg_queue = SqlitePegQueue::in_memory(Some(1), 2).await.unwrap();
        let number_of_simulated_blocks: u64 = 3;

        let stacks_node_mock = default_stacks_node_mock(number_of_simulated_blocks);

        // Fast forward past first poll
        peg_queue.poll(&stacks_node_mock).await.unwrap();
        for _ in 1..=number_of_simulated_blocks {
            peg_queue.sbtc_op().await.unwrap().unwrap();
            peg_queue.sbtc_op().await.unwrap().unwrap();
        }

        let mut stacks_node_mock = stacks_node::MockStacksNode::new();

        stacks_node_mock
            .expect_burn_block_height()
            .returning(move || Ok(number_of_simulated_blocks));

        stacks_node_mock.expect_get_peg_in_ops().never();
        stacks_node_mock.expect_get_peg_out_request_ops().never();

        peg_queue.poll(&stacks_node_mock).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn calling_poll_should_find_new_ops_if_at_new_block_height() {
        let peg_queue = SqlitePegQueue::in_memory(Some(1), 2).await.unwrap();
        let number_of_simulated_blocks: u64 = 3;
        let number_of_simulated_blocks_second_poll: u64 = 5;

        let stacks_node_mock = default_stacks_node_mock(number_of_simulated_blocks);

        // Fast forward past first poll
        peg_queue.poll(&stacks_node_mock).await.unwrap();
        for _ in 1..=number_of_simulated_blocks {
            peg_queue.sbtc_op().await.unwrap().unwrap();
            peg_queue.sbtc_op().await.unwrap().unwrap();
        }

        let stacks_node_mock = default_stacks_node_mock(number_of_simulated_blocks_second_poll);
        peg_queue.poll(&stacks_node_mock).await.unwrap();

        for height in number_of_simulated_blocks + 1..=number_of_simulated_blocks_second_poll {
            let next_op = peg_queue.sbtc_op().await.unwrap().unwrap();
            assert!(next_op.as_peg_in().is_some());
            assert_eq!(next_op.as_peg_in().unwrap().block_height, height);

            let next_op = peg_queue.sbtc_op().await.unwrap().unwrap();
            assert!(next_op.as_peg_out_request().is_some());
            assert_eq!(next_op.as_peg_out_request().unwrap().block_height, height);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn acknowledged_entries_should_have_acknowledge_status() {
        let peg_queue = SqlitePegQueue::in_memory(Some(1), 2).await.unwrap();
        let number_of_simulated_blocks: u64 = 1;

        let stacks_node_mock = default_stacks_node_mock(number_of_simulated_blocks);
        peg_queue.poll(&stacks_node_mock).await.unwrap();

        let next_op = peg_queue.sbtc_op().await.unwrap().unwrap();
        let peg_in_op = next_op.as_peg_in().unwrap();
        peg_queue
            .acknowledge(&peg_in_op.txid, &peg_in_op.burn_header_hash)
            .await
            .unwrap();

        let entry = peg_queue
            .get_entry(&peg_in_op.txid, &peg_in_op.burn_header_hash)
            .await
            .unwrap();

        assert_eq!(entry.status, Status::Acknowledged);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn should_start_at_last_observed_block_height_when_polling() {
        let start_block_height: u64 = 10;
        let initial_node_block_height: u64 = 20;
        let second_poll_node_block_height: u64 = 40;

        let peg_queue =
            SqlitePegQueue::in_memory(Some(start_block_height), initial_node_block_height)
                .await
                .unwrap();

        let stacks_node_mock = default_stacks_node_mock(initial_node_block_height);
        peg_queue.poll(&stacks_node_mock).await.unwrap();

        assert_eq!(peg_queue.last_processed_block_height().await.unwrap(), 20);

        let stacks_node_mock = stacks_node_mock_with_no_sbtc_ops(second_poll_node_block_height);
        peg_queue.poll(&stacks_node_mock).await.unwrap();

        assert_eq!(peg_queue.last_processed_block_height().await.unwrap(), 40);
    }

    fn default_stacks_node_mock(block_height: u64) -> stacks_node::MockStacksNode {
        let mut stacks_node_mock = stacks_node::MockStacksNode::new();

        stacks_node_mock
            .expect_burn_block_height()
            .returning(move || Ok(block_height));

        stacks_node_mock
            .expect_get_peg_in_ops()
            .returning(|height| Ok(vec![peg_in_op(height)]));

        stacks_node_mock
            .expect_get_peg_out_request_ops()
            .returning(|height| Ok(vec![peg_out_request_op(height)]));

        stacks_node_mock
    }

    fn stacks_node_mock_with_no_sbtc_ops(block_height: u64) -> stacks_node::MockStacksNode {
        let mut stacks_node_mock = stacks_node::MockStacksNode::new();

        stacks_node_mock
            .expect_burn_block_height()
            .returning(move || Ok(block_height));

        stacks_node_mock
            .expect_get_peg_in_ops()
            .returning(|_height| Ok(vec![]));

        stacks_node_mock
            .expect_get_peg_out_request_ops()
            .returning(|_height| Ok(vec![]));

        stacks_node_mock
    }

    fn peg_in_op(block_height: u64) -> PegInOp {
        let recipient_stx_addr = StacksAddress::new(26, Hash160([0; 20]));
        let peg_wallet_address =
            PoxAddress::Standard(StacksAddress::new(0, Hash160([0; 20])), None);

        PegInOp {
            recipient: recipient_stx_addr.into(),
            peg_wallet_address,
            amount: 1337,
            memo: vec![1, 3, 3, 7],
            txid: Txid(hash_and_expand(block_height, 1)),
            burn_header_hash: BurnchainHeaderHash(hash_and_expand(block_height, 0)),
            block_height,
            vtxindex: 0,
        }
    }

    fn peg_out_request_op(block_height: u64) -> PegOutRequestOp {
        let recipient_stx_addr = StacksAddress::new(26, Hash160([0; 20]));
        let peg_wallet_address =
            PoxAddress::Standard(StacksAddress::new(0, Hash160([0; 20])), None);

        stacks_node::PegOutRequestOp {
            recipient: PoxAddress::Standard(recipient_stx_addr, None),
            peg_wallet_address,
            amount: 1337,
            fulfillment_fee: 1000,
            signature: MessageSignature([0; 65]),
            memo: vec![1, 3, 3, 7],
            txid: Txid(hash_and_expand(block_height, 2)),
            burn_header_hash: BurnchainHeaderHash(hash_and_expand(block_height, 0)),
            block_height,
            vtxindex: 0,
        }
    }

    fn hash_and_expand(val: u64, nonce: u64) -> [u8; 32] {
        let mut hasher = DefaultHasher::new();
        hasher.write_u64(val);
        hasher.write_u64(nonce);
        let hash = hasher.finish();

        hash.to_be_bytes().repeat(4).try_into().unwrap()
    }
}
