// SPDX-License-Identifier: MIT OR Apache-2.0
#![deny(clippy::unwrap_used)]

use bitcoin::Amount;
use bitcoin::BlockHash;
use bitcoin::OutPoint;
use bitcoin::TxOut;

#[derive(Debug, Clone)]
pub struct GetBalanceParams {
    /// Only include transactions confirmed at least this many times (default: 0)
    pub minconf: u32,

    /// Exclude dirty outputs from balance calculation (default: true)
    /// Only available if avoid_reuse wallet flag is set
    pub avoid_reuse: bool,
}

impl Default for GetBalanceParams {
    fn default() -> Self {
        Self {
            minconf: 0,
            avoid_reuse: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LastProcessedBlock {
    /// Hash of the block this balance was generated on
    pub hash: BlockHash,
    /// Height of the block this balance was generated on
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct Balance {
    // trusted balance (outputs created by the wallet or confirmed outputs)
    pub trusted: Amount,

    // untrusted pending balance (outputs created by others that are in the mempool)
    pub untrusted_pending: Amount,

    // balance from immature coinbase outputs
    pub immature: Amount,

    // (optional) (only present if avoid_reuse is set) balance from coins sent to addresses that were
    // previously spent from (potentially privacy violating)
    pub used: Option<Amount>,

    pub last_processed_block: LastProcessedBlock,
}

impl Balance {
    pub fn total(&self) -> Amount {
        self.trusted + self.untrusted_pending + self.immature
    }

    pub fn trusted_spendable(&self) -> Amount {
        self.trusted
    }
}

#[derive(Debug, Clone)]
pub struct LocalOutput {
    pub outpoint: OutPoint,
    pub txout: TxOut,
    pub is_spent: bool,
}

#[derive(Debug, Clone)]
pub struct ImportDescriptor {
    pub descriptor: String,
    pub label: Option<String>,
    pub is_active: bool,
    pub is_change: bool,
}
