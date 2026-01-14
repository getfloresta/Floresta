use floresta_wire::mempool::AcceptToMempoolError;
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
    Mempool(AcceptToMempoolError),

    #[error("Node isn't working")]
    NodeInterface(#[from] oneshot::error::RecvError),
}
