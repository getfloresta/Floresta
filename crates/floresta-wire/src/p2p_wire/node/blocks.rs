use std::time::Instant;

use bitcoin::p2p::ServiceFlags;
use bitcoin::Block;
use bitcoin::BlockHash;
use floresta_chain::proof_util;
use floresta_chain::proof_util::UtreexoLeafError;
use floresta_chain::BlockValidationErrors;
use floresta_chain::BlockchainError;
use floresta_chain::ChainBackend;
use floresta_chain::CompactLeafData;
use floresta_common::service_flags;
use floresta_common::service_flags::UTREEXO;
use rustreexo::accumulator::proof::Proof;
use tracing::debug;
use tracing::error;
use tracing::warn;

use super::try_and_log;
use super::InflightRequests;
use super::NodeRequest;
use super::UtreexoNode;
use crate::address_man::AddressState;
use crate::block_proof::Bitmap;
use crate::block_proof::UtreexoProof;
use crate::node_context::NodeContext;
use crate::node_context::PeerId;
use crate::p2p_wire::error::WireError;

/// The leaf data, utreexo proof and the peer that sent them.
type UtreexoData = (Vec<CompactLeafData>, Proof, PeerId);

#[derive(Debug)]
/// A block that is currently being downloaded or pending processing
///
/// To download a block, we first request the block itself, and then we
/// request the proof and leaf data for it. This struct holds the data
/// we already have. We may also keep it around, as we may receive blocks
/// out of order, so while we wait for the previous blocks to finish download,
/// we keep the blocks that are already downloaded as an [`InflightBlock`].
pub(crate) struct InflightBlock {
    /// The peer that sent the block.
    pub peer: PeerId,

    /// The block itself.
    pub block: Block,

    /// Auxiliary data needed for validating this block.
    pub aux_data: Option<UtreexoData>,
}

impl InflightBlock {
    /// Creates a new `InflightBlock` from a block and the associated peer id.
    fn new(block: Block, peer: PeerId) -> Self {
        Self {
            peer,
            block,
            aux_data: None,
        }
    }

    /// Attaches the auxiliary utreexo data to this `InflightBlock`.
    fn add_utreexo_data(&mut self, leaf_data: Vec<CompactLeafData>, proof: Proof, peer: PeerId) {
        self.aux_data = Some((leaf_data, proof, peer));
    }
}

impl<T, Chain> UtreexoNode<Chain, T>
where
    T: 'static + Default + NodeContext,
    Chain: ChainBackend + 'static,
    WireError: From<Chain::Error>,
{
    pub(crate) fn request_blocks(&mut self, blocks: Vec<BlockHash>) -> Result<(), WireError> {
        let should_request = |block: &BlockHash| {
            let is_inflight = self
                .inflight
                .contains_key(&InflightRequests::Blocks(*block));
            let is_pending = self.blocks.contains_key(block);

            !(is_inflight || is_pending)
        };

        let blocks: Vec<_> = blocks.into_iter().filter(should_request).collect();
        // if there's no block to request, don't propagate any message
        if blocks.is_empty() {
            return Ok(());
        }

        let peer =
            self.send_to_fast_peer(NodeRequest::GetBlock(blocks.clone()), ServiceFlags::NETWORK)?;

        for block in blocks.iter() {
            self.inflight
                .insert(InflightRequests::Blocks(*block), (peer, Instant::now()));
        }

        Ok(())
    }

    pub(crate) fn request_block_proof(
        &mut self,
        block: Block,
        peer: PeerId,
    ) -> Result<(), WireError> {
        let block_hash = block.block_hash();
        self.inflight.remove(&InflightRequests::Blocks(block_hash));

        // Reply and return early if it's a user-requested block. Else continue handling it.
        let Some(block) = self.check_is_user_block_and_reply(block)? else {
            return Ok(());
        };

        if block.txdata.len() == 1 {
            let mut inflight_block = InflightBlock::new(block, peer);

            // This is an empty block, so there's no proof nor leaf data for it
            inflight_block.add_utreexo_data(Vec::new(), Proof::default(), peer);

            self.blocks.insert(block_hash, inflight_block);
            return Ok(());
        }

        debug!("Received block {block_hash} from peer {peer}");

        self.send_to_fast_peer(
            NodeRequest::GetBlockProof((block_hash, Bitmap::new(), Bitmap::new())),
            UTREEXO.into(),
        )?;

        self.inflight.insert(
            InflightRequests::UtreexoProof(block_hash),
            (peer, Instant::now()),
        );

        self.blocks
            .insert(block_hash, InflightBlock::new(block, peer));

        Ok(())
    }

    pub(crate) fn attach_proof(
        &mut self,
        uproof: UtreexoProof,
        peer: PeerId,
    ) -> Result<(), WireError> {
        debug!("Received utreexo proof for block {}", uproof.block_hash);
        self.inflight
            .remove(&InflightRequests::UtreexoProof(uproof.block_hash));

        let Some(block) = self.blocks.get_mut(&uproof.block_hash) else {
            warn!(
                "Received utreexo proof for block {}, but we don't have it",
                uproof.block_hash
            );
            self.increase_banscore(peer, 5)?;

            return Ok(());
        };

        let proof = Proof {
            hashes: uproof.proof_hashes,
            targets: uproof.targets,
        };

        // Add the proof and leaf data, together with the peer id that sent them
        block.add_utreexo_data(uproof.leaf_data, proof, peer);

        Ok(())
    }

    /// Asks all utreexo peers for proofs of blocks that we have, but haven't received proofs
    /// for yet, and don't have any GetProofs inflight. This may be caused by a peer disconnecting
    /// while we didn't have more utreexo peers to redo the request.
    pub(crate) fn ask_for_missed_proofs(&mut self) -> Result<(), WireError> {
        // If we have no peers, we can't ask for proofs
        if !self.has_utreexo_peers() {
            return Ok(());
        }

        let pending_blocks = self
            .blocks
            .iter()
            .filter_map(|(hash, block)| {
                if block.aux_data.is_some() {
                    return None;
                }

                if !self
                    .inflight
                    .contains_key(&InflightRequests::UtreexoProof(*hash))
                {
                    return Some(*hash);
                }

                None
            })
            .collect::<Vec<_>>();

        for block_hash in pending_blocks {
            let peer = self.send_to_fast_peer(
                NodeRequest::GetBlockProof((block_hash, Bitmap::new(), Bitmap::new())),
                service_flags::UTREEXO.into(),
            )?;

            self.inflight.insert(
                InflightRequests::UtreexoProof(block_hash),
                (peer, Instant::now()),
            );
        }

        Ok(())
    }

    /// Processes ready blocks in order, stopping at the tip or the first missing block/proof.
    /// Call again when new blocks or proofs arrive.
    pub(crate) fn process_pending_blocks(&mut self) -> Result<(), WireError>
    where
        Chain::Error: From<UtreexoLeafError>,
    {
        loop {
            let best_block = self.chain.get_best_block()?.0;
            let next_block = self.chain.get_validation_index()? + 1;
            if next_block > best_block {
                // If we are at the best block, we don't need to process any more blocks
                return Ok(());
            }

            let next_block_hash = self.chain.get_block_hash(next_block)?;

            let Some(block) = self.blocks.get(&next_block_hash) else {
                // If we don't have the next block, we can't process it
                return Ok(());
            };

            if block.aux_data.is_none() {
                // If the block doesn't have a proof, we can't process it
                return Ok(());
            }

            let start = Instant::now();
            self.process_block(next_block, next_block_hash)?;

            let elapsed = start.elapsed().as_secs_f64();
            self.block_sync_avg.add(elapsed);

            #[cfg(feature = "metrics")]
            {
                use metrics::get_metrics;

                let avg = self.block_sync_avg.value().expect("at least one sample");
                let metrics = get_metrics();
                metrics.avg_block_processing_time.set(avg);
            }
        }
    }

    /// Actually process a block that is ready to be processed.
    ///
    /// This function will take the next block in our chain, process its proof and validate it.
    /// If everything is correct, it will connect the block to our chain.
    fn process_block(&mut self, block_height: u32, block_hash: BlockHash) -> Result<(), WireError>
    where
        Chain::Error: From<UtreexoLeafError>,
    {
        debug!("processing block {block_hash}");

        let inflight = self
            .blocks
            .remove(&block_hash)
            .ok_or(WireError::BlockNotFound)?;

        let block = inflight.block;
        let peer = inflight.peer;
        let (leaf_data, proof, utreexo_peer) =
            inflight.aux_data.ok_or(WireError::BlockProofNotFound)?;

        let (del_hashes, inputs) =
            proof_util::process_proof(&leaf_data, &block.txdata, block_height, |h| {
                self.chain.get_block_hash(h)
            })?;

        if let Err(chain_err) = self.chain.connect_block(&block, proof, inputs, del_hashes) {
            error!(
                "Invalid block {:?} received by peer {} reason: {:?}",
                block.header, peer, chain_err,
            );

            if let Some(e) = Self::block_validation_err(chain_err) {
                // Because the proof isn't committed to the block, we can't invalidate
                // it if the proof is invalid. Any other error should cause the block
                // to be invalidated.
                match e {
                    BlockValidationErrors::InvalidCoinbase(_)
                    | BlockValidationErrors::UtxoNotFound(_)
                    | BlockValidationErrors::ScriptValidationError(_)
                    | BlockValidationErrors::NullPrevOut
                    | BlockValidationErrors::EmptyInputs
                    | BlockValidationErrors::EmptyOutputs
                    | BlockValidationErrors::ScriptError
                    | BlockValidationErrors::BlockTooBig
                    | BlockValidationErrors::NotEnoughPow
                    | BlockValidationErrors::TooManyCoins
                    | BlockValidationErrors::BadMerkleRoot
                    | BlockValidationErrors::BadWitnessCommitment
                    | BlockValidationErrors::NotEnoughMoney
                    | BlockValidationErrors::FirstTxIsNotCoinbase
                    | BlockValidationErrors::BadCoinbaseOutValue
                    | BlockValidationErrors::EmptyBlock
                    | BlockValidationErrors::BadBip34
                    | BlockValidationErrors::BIP94TimeWarp
                    | BlockValidationErrors::UnspendableUTXO
                    | BlockValidationErrors::CoinbaseNotMatured => {
                        try_and_log!(self.chain.invalidate_block(block.block_hash()));
                    }
                    BlockValidationErrors::InvalidProof => {}
                    BlockValidationErrors::BlockExtendsAnOrphanChain
                    | BlockValidationErrors::BlockDoesntExtendTip => {
                        // for some reason, we've tried to connect a block that doesn't extend the
                        // tip
                        self.last_block_request = self.chain.get_validation_index().unwrap_or(0);
                        // this is our mistake, don't punish the peer
                        return Ok(());
                    }
                }
            }

            warn!(
                "Block {} from peer {peer} is invalid, banning peer",
                block.block_hash()
            );

            // Disconnect the peer and ban it.
            if let Some(peer) = self.peers.get(&peer).cloned() {
                self.address_man
                    .update_set_state(peer.address_id as usize, AddressState::Banned(T::BAN_TIME));
            }

            self.send_to_peer(peer, NodeRequest::Shutdown)?;
            return Err(WireError::PeerMisbehaving);
        }

        self.last_tip_update = Instant::now();
        Ok(())
    }

    /// Returns the inner `BlockValidationErrors`, if any.
    fn block_validation_err(e: BlockchainError) -> Option<BlockValidationErrors> {
        match e {
            BlockchainError::TransactionError(tx_err) => Some(tx_err.error),
            BlockchainError::BlockValidation(block_err) => Some(block_err),
            // TODO: we need clearer error definitions for utreexo failures
            BlockchainError::UtreexoError(_) | BlockchainError::InvalidProof => {
                Some(BlockValidationErrors::InvalidProof)
            }
            _ => None,
        }
    }
}
