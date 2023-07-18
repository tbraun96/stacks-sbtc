CREATE TABLE IF NOT EXISTS config (
    id INTEGER NOT NULL PRIMARY KEY,
    secret_key TEXT NOT NULL,
    auto_approve_max_amount INTEGER NOT NULL,
    delegate_public_key TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS delegator_public_keys (
    public_key TEXT NOT NULL PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS auto_deny_addresses (
    address TEXT NOT NULL PRIMARY KEY
);

CREATE TABLE IF NOT EXISTS transactions (
    txid TEXT NOT NULL PRIMARY KEY,
    transaction_kind TEXT NOT NULL,
    transaction_block_height INTEGER,
    transaction_deadline_block_height INTEGER NOT NULL,
    transaction_amount INTEGER NOT NULL,
    transaction_fees INTEGER NOT NULL,
    memo BLOB NOT NULL,
    transaction_originator_address TEXT NOT NULL,
    transaction_debit_address TEXT NOT NULL,
    transaction_credit_address TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS votes (
    txid TEXT NOT NULL PRIMARY KEY,
    vote_status TEXT NOT NULL,
    vote_choice TEXT,
    vote_mechanism TEXT NOT NULL,
    target_consensus INTEGER NOT NULL,
    current_consensus INTEGER NOT NULL,

    FOREIGN KEY(txid) REFERENCES transactions(txid) ON DELETE CASCADE
);
CREATE TRIGGER add_empty_vote
    AFTER INSERT ON transactions
    FOR EACH ROW
        WHEN NEW.txid NOT IN (SELECT txid FROM votes)
        BEGIN
            INSERT INTO votes (
                txid, vote_status, vote_choice, vote_mechanism, target_consensus, current_consensus
            ) VALUES (
                NEW.txid, 'pending', NULL, 'manual', 70, 0
            );
        END;