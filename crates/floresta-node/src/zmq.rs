// SPDX-License-Identifier: MIT OR Apache-2.0

//! A small Zero Message Queue (ZMQ) implementation for floresta, that pushes new blocks
//! as they are found.
//!
//! # Examples
//! Creating a server
//! ```ignore
//! use florestad::zmq::ZMQServer;
//! let _ = ZMQServer::new("tcp://127.0.0.1:5150");
//! ```
//!
//! Listening for new blocks
//!
//! ```ignore
//! use zmq::{Context, Socket};
//! let ctx =  Context::new();
//! // The opposite of PUSH is PULL
//! let socket = ctx.socket(zmq::SocketType::PULL).unwrap();
//!
//! socket.connect(addr).unwrap();
//! let block = socket.recv().unwrap();
//! ```

use std::collections::HashMap;

use bitcoin::consensus::serialize;
use bitcoin::OutPoint;
use floresta_chain::BlockConsumer;
use floresta_chain::UtxoData;
use tracing::error;
use zmq::Context;
use zmq::Socket;

/// A 0MQ server that pushes blocks when we receive them
pub struct ZMQServer {
    /// The ZMQ context that holds our socket. We don't really need it,
    /// but if _ctx gets dropped, the socket is closed, so we keep it here.
    _ctx: Context,
    /// The actual socket where we'll send blocks
    socket: Socket,
}

/// # Safety
/// We only keep one reference of ZMQServer, usually inside an [std::sync::Arc], so it's ok
/// to assume it can be [Sync].
unsafe impl Sync for ZMQServer {}

impl ZMQServer {
    /// Creates a new ZMQ server that listens on `addr` and pushes blocks
    /// to connected peers as they are accepted
    pub fn new(addr: &str) -> Result<ZMQServer, zmq::Error> {
        let ctx = Context::new();
        let socket = ctx.socket(zmq::SocketType::PUSH)?;
        socket.bind(addr)?;
        Ok(ZMQServer { _ctx: ctx, socket })
    }
}

// Implement BlockConsumer so we can subscribe on `ChainState`
impl BlockConsumer for ZMQServer {
    fn wants_spent_utxos(&self) -> bool {
        false
    }

    fn on_block(
        &self,
        block: &bitcoin::Block,
        _height: u32,
        _spent_utxos: Option<&HashMap<OutPoint, UtxoData>>,
    ) {
        let block = serialize(&block);
        if let Err(e) = self.socket.send(block, zmq::DONTWAIT) {
            error!("while sending block over zmq: {e}");
        }
    }
}
