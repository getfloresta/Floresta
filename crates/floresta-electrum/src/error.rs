// SPDX-License-Identifier: MIT OR Apache-2.0

use thiserror::Error;
use tokio::sync::oneshot;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid params passed in")]
    InvalidParams,

    #[error("BitAssets support is not enabled for this Electrum server")]
    BitAssetsUnavailable,

    #[error("BitAssets sidechain RPC is not configured for this Electrum server")]
    BitAssetsRpcUnavailable,

    #[error("BitAssets sidechain RPC error: {0}")]
    BitAssetsRpc(String),

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
