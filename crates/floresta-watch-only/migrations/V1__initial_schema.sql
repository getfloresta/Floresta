-- Create wallets table
CREATE TABLE IF NOT EXISTS wallets (
    name TEXT PRIMARY KEY
);

-- Create descriptors table
CREATE TABLE IF NOT EXISTS descriptors (
    wallet_id TEXT NOT NULL,
    id TEXT NOT NULL,
    descriptor TEXT NOT NULL,
    label TEXT,
    is_active BOOLEAN NOT NULL,
    is_change BOOLEAN NOT NULL,
    PRIMARY KEY (wallet_id, id),
    FOREIGN KEY (wallet_id) REFERENCES wallets(name) ON DELETE CASCADE
);

-- Create transactions table
CREATE TABLE IF NOT EXISTS transactions (
    hash TEXT PRIMARY KEY,
    tx BLOB NOT NULL,
    height INTEGER,
    merkle_block BLOB,
    position INTEGER
);

-- Create script_buffers table
CREATE TABLE IF NOT EXISTS script_buffers (
    hash TEXT PRIMARY KEY,
    script BLOB NOT NULL UNIQUE
);