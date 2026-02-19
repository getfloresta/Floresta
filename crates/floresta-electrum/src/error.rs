use floresta_common::impl_error_from;
use floresta_watch_only::sqlite_database::SqliteDatabaseError;
use floresta_watch_only::WatchOnlyError;
use thiserror::Error;
use tokio::sync::oneshot;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid params passed in")]
    InvalidParams,

    #[error("Invalid json string {0}")]
    Parsing(#[from] serde_json::Error),

    #[error("Blockchain error")]
    Blockchain(Box<dyn floresta_common::prelude::Error + Send + 'static>),

    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("Mempool accept error")]
    Mempool(Box<dyn floresta_common::prelude::Error + Send + 'static>),

    #[error("Node isn't working")]
    NodeInterface(#[from] oneshot::error::RecvError),

    #[error("Wallet error: {0}")]
    Wallet(WatchOnlyError<SqliteDatabaseError>),
}

impl_error_from!(Error, WatchOnlyError<SqliteDatabaseError>, Wallet);
