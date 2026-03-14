// SPDX-License-Identifier: MIT OR Apache-2.0

use thiserror::Error;
use tokio::sync::oneshot;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid params passed in")]
    InvalidParams,

    #[error("Invalid json string {0}")]
    Parsing(#[from] serde_json::Error),

    #[error("Blockchain error")]
    Blockchain(Box<dyn core::error::Error + Send + 'static>),

    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("Mempool accept error")]
    Mempool(Box<dyn core::error::Error + Send + 'static>),

    #[error("Node isn't working")]
    NodeInterface(#[from] oneshot::error::RecvError),
}
