// SPDX-License-Identifier: MIT OR Apache-2.0

//! In node.rs we created a node that connects to the Bitcoin network and downloads the blockchain.
//! We use the default chainstate, which starts at genesis and validates all blocks, but you can
//! customized the chainstate to subscribe to all new blocks, including the input UTXO set. This
//! is useful for indexing metaprotocols, tracking recent fee rates, and detecting new spends from
//! wallets with unknown balances.

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use bitcoin::Block;
use bitcoin::Network;
use bitcoin::OutPoint;
use floresta_chain::AssumeValidArg;
use floresta_chain::BlockConsumer;
use floresta_chain::BlockchainInterface;
use floresta_chain::ChainState;
use floresta_chain::FlatChainStore;
use floresta_chain::FlatChainStoreConfig;
use floresta_chain::UtxoData;

const DATA_DIR: &str = "./tmp-db";

/// A sample indexer that tracks the min fee rate of the last block
struct FeeRateIndexer {
    /// The median fee rate of the last block (sats / kvb)
    min_sats_per_kvb: AtomicU64,
}

impl FeeRateIndexer {
    pub fn new() -> Self {
        FeeRateIndexer {
            min_sats_per_kvb: AtomicU64::new(0),
        }
    }
}

// Implement BlockConsumer so we can subscribe on `ChainState`
impl BlockConsumer for FeeRateIndexer {
    fn wants_spent_utxos(&self) -> bool {
        true
    }

    fn on_block(
        &self,
        block: &Block,
        _height: u32,
        spent_utxos: Option<&HashMap<OutPoint, UtxoData>>,
    ) {
        // Calculate minimum fee rate, ignoring coinbase transaction
        let spent_utxos = spent_utxos.expect("Safe to unwrap because wants_spent_utxos()");
        let min_fee_rate = block
            .txdata
            .iter()
            .skip(1)
            .map(|tx| {
                let total_in: u64 = tx
                    .input
                    .iter()
                    .filter_map(|txin| {
                        spent_utxos
                            .get(&txin.previous_output)
                            .map(|utxo| utxo.txout.value.to_sat())
                    })
                    .sum();

                let total_out: u64 = tx.output.iter().map(|txout| txout.value.to_sat()).sum();

                let fee = total_in - total_out;
                let weight = tx.weight().to_wu();
                fee * 4000 / weight
            })
            .min()
            .unwrap_or(0);

        self.min_sats_per_kvb.store(min_fee_rate, Ordering::Relaxed);
    }
}

#[tokio::main]
async fn main() {
    // Create a new chain state, which will store the accumulator and the headers chain.
    // It will be stored in the DATA_DIR directory. With this chain state, we don't keep
    // the block data after we have validated it. This saves a lot of space, but it means that
    // we can't serve blocks to other nodes or rescan the blockchain without downloading
    // it again.
    let chain_store_config = FlatChainStoreConfig::new(DATA_DIR.into());
    let chain_store =
        FlatChainStore::new(chain_store_config).expect("failed to open the blockchain database");

    // The actual chainstate. It will keep track of the current state of the accumulator
    // and the headers chain. It will also validate new blocks and headers as we receive them.
    // The last parameter is the assume valid block. We assume that all blocks before this
    // one have valid signatures. This is a performance optimization, as we don't need to validate all
    // signatures in the blockchain, just the ones after the assume valid block. We are giving a Disabled
    // value, so we will validate all signatures regardless.
    // We place the chain state in an Arc, so we can share it with other components.
    let chain = Arc::new(ChainState::new(
        chain_store,
        Network::Bitcoin,
        AssumeValidArg::Disabled,
    ));

    // Create the indexer and subscribe to new blocks with the spent UTXOs
    let indexer = FeeRateIndexer::new();
    chain.subscribe(Arc::new(indexer));

    // ... If you want to drive the chainstate, you can use the BlockchainInterface trait.
    // See node.rs for an example on how to do it ...
}
