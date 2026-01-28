use core::cmp::min;
use core::ops::Add;

use bitcoin::block::Header;
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::BlockHash;
use bitcoin::Work;
use floresta_common::prelude::Box;
use floresta_common::prelude::Error;
use floresta_common::prelude::String;
use floresta_common::prelude::Vec;

use crate::BlockchainInterface;

const MEDIAN_TIME_PAST_BLOCK_COUNT: usize = 11;

/// Provides additional methods for working with [`Header`] objects,
pub trait HeaderExt {
    /// Calculates the Median Time Past (MTP) for the block.
    fn calculate_median_time_past(
        &self,
        chain: &impl BlockchainInterface,
    ) -> Result<u32, HeaderExtError>;

    /// Calculates the total accumulated chain work up to the current block.
    fn calculate_chain_work(
        &self,
        chain: &impl BlockchainInterface,
    ) -> Result<Work, HeaderExtError>;

    /// Retrieves the hash of the next block in the chain, if it exists.
    ///
    /// Returns `None` if the block is the tip of the chain.
    fn get_next_block_hash(
        &self,
        chain: &impl BlockchainInterface,
    ) -> Result<Option<BlockHash>, HeaderExtError>;

    /// Retrieves the header of the previous block in the chain.
    fn get_previous_block_header(
        &self,
        chain: &impl BlockchainInterface,
    ) -> Result<Header, HeaderExtError>;

    /// Returns the block's "bits" field as a hexadecimal string.
    fn get_bits_hex(&self) -> String;

    /// Calculates the number of confirmations for the current block.
    fn get_confirmations(&self, chain: &impl BlockchainInterface) -> Result<u32, HeaderExtError>;

    /// Returns the block's difficulty as a floating-point number.
    fn get_difficulty(&self) -> f64;

    /// Retrieves the height of the block in the blockchain.
    fn get_height(&self, chain: &impl BlockchainInterface) -> Result<u32, HeaderExtError>;

    /// Returns the block's target as a hexadecimal string.
    ///
    /// In `rust-bitcoin`, calling `to_string` on `Target` returns the value in decimal
    /// because it wraps a `U256`, which defaults to decimal string conversion. However,
    /// Bitcoin Core represents targets in hexadecimal. This method ensures the target
    /// is returned in hexadecimal format, consistent with Bitcoin Core.
    fn get_target_hex(&self) -> String;

    /// Returns the block's version as a hexadecimal string.
    ///
    /// Bitcoin Core represents the block version as a 32-bit unsigned integer (`u32`)
    /// in hexadecimal format. This method ensures the version is returned as a
    /// properly formatted hexadecimal string, consistent with Bitcoin Core.
    fn get_version_hex(&self) -> String;
}

/// Errors that can occur when using the `HeaderExt` methods.
#[derive(Debug)]
pub enum HeaderExtError {
    /// An error related to the blockchain interface, wrapping the actual error.
    Chain(Box<dyn Error + Send + Sync>),

    /// Indicates that the block could not be found in the blockchain.
    BlockNotFound,

    /// You got an overflow while calculating the chain work.
    ChainWorkOverflow,
}

impl core::fmt::Display for HeaderExtError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HeaderExtError::Chain(e) => write!(f, "chain error: {}", e),
            HeaderExtError::BlockNotFound => write!(f, "block not found"),
            HeaderExtError::ChainWorkOverflow => write!(f, "chain work overflow"),
        }
    }
}

impl HeaderExt for Header {
    fn calculate_median_time_past(
        &self,
        chain: &impl BlockchainInterface,
    ) -> Result<u32, HeaderExtError> {
        let mut block_timestamps = Vec::with_capacity(MEDIAN_TIME_PAST_BLOCK_COUNT);
        let mut current_header = *self;
        for _ in 0..MEDIAN_TIME_PAST_BLOCK_COUNT {
            block_timestamps.push(current_header.time);
            let Ok(prev_header) = current_header.get_previous_block_header(chain) else {
                break;
            };
            current_header = prev_header;
        }
        block_timestamps.sort();
        let median_time_past = block_timestamps[block_timestamps.len() / 2];

        Ok(median_time_past)
    }

    fn calculate_chain_work(
        &self,
        chain: &impl BlockchainInterface,
    ) -> Result<Work, HeaderExtError> {
        let block_height = self.get_height(chain)?;

        let mut total_chainwork = Work::from_be_bytes([0u8; 32]);
        for epoch_start_height in (0..=block_height).step_by(2016) {
            // Calculate the number of blocks in this epoch
            let epoch_end_height = min(epoch_start_height + 2015, block_height);
            let blocks_in_epoch = epoch_end_height - epoch_start_height + 1;

            // Get the block hash and header at the start of the epoch
            let epoch_block_hash = chain
                .get_block_hash(epoch_start_height)
                .map_err(|e| HeaderExtError::Chain(Box::new(e)))?;
            let epoch_block_header = chain
                .get_block_header(&epoch_block_hash)
                .map_err(|e| HeaderExtError::Chain(Box::new(e)))?;

            let epoch_chainwork = epoch_block_header
                .work()
                .multiply_work_by_u32(blocks_in_epoch)?;
            total_chainwork = total_chainwork.add(epoch_chainwork);
        }

        Ok(total_chainwork)
    }

    fn get_next_block_hash(
        &self,
        chain: &impl BlockchainInterface,
    ) -> Result<Option<BlockHash>, HeaderExtError> {
        let height = self.get_height(chain)?;

        // If obtaining the next block hash fails, treat it as "no next block" and return Ok(None)
        match chain.get_block_hash(height + 1) {
            Ok(opt_hash) => Ok(Some(opt_hash)),
            Err(_) => Ok(None),
        }
    }

    fn get_previous_block_header(
        &self,
        chain: &impl BlockchainInterface,
    ) -> Result<Header, HeaderExtError> {
        let prev_header = chain
            .get_block_header(&self.prev_blockhash)
            .map_err(|e| HeaderExtError::Chain(Box::new(e)))?;
        Ok(prev_header)
    }

    fn get_bits_hex(&self) -> String {
        serialize_hex(&self.bits.to_consensus().to_be())
    }

    fn get_confirmations(&self, chain: &impl BlockchainInterface) -> Result<u32, HeaderExtError> {
        let height = self.get_height(chain)?;

        let chain_height = chain
            .get_height()
            .map_err(|e| HeaderExtError::Chain(Box::new(e)))?;

        Ok(chain_height - height + 1)
    }

    fn get_difficulty(&self) -> f64 {
        self.difficulty_float()
    }

    fn get_height(&self, chain: &impl BlockchainInterface) -> Result<u32, HeaderExtError> {
        let height = match chain.get_block_height(&self.block_hash()) {
            Ok(Some(height)) => height,
            Ok(None) => return Err(HeaderExtError::BlockNotFound),
            Err(e) => return Err(HeaderExtError::Chain(Box::new(e))),
        };

        Ok(height)
    }

    fn get_target_hex(&self) -> String {
        serialize_hex(&self.target().to_be_bytes())
    }

    fn get_version_hex(&self) -> String {
        serialize_hex(&(self.version.to_consensus() as u32).to_be())
    }
}

impl From<ChainWorkOverflow> for HeaderExtError {
    fn from(_: ChainWorkOverflow) -> Self {
        Self::ChainWorkOverflow
    }
}

#[derive(Debug, PartialEq)]
pub struct ChainWorkOverflow;

pub trait WorkExt {
    /// Multiplies the Work by a u32 factor, returning an error if overflow occurs.
    fn multiply_work_by_u32(self, factor: u32) -> Result<Work, ChainWorkOverflow>;

    /// Returns the hexadecimal string representation of the Work.
    ///
    /// In `rust-bitcoin`, calling `to_string` on `Work` returns the value in decimal
    /// because it wraps a `U256`, which defaults to decimal string conversion. However,
    /// Bitcoin Core represents targets in hexadecimal. This method ensures the `Work``
    /// is returned in hexadecimal format, consistent with Bitcoin Core.
    fn to_string_hex(&self) -> String;
}

impl WorkExt for Work {
    fn multiply_work_by_u32(self, factor: u32) -> Result<Work, ChainWorkOverflow> {
        if factor == 0 {
            return Ok(Work::from_be_bytes([0u8; 32]));
        }

        if factor == 1 {
            return Ok(self);
        }

        // Convert Work to little-endian bytes for easier manipulation (least significant byte first)
        let work_bytes = self.to_le_bytes();
        let mut carry_high: u64 = 0;
        let mut result_bytes = [0u8; 32];
        let word_size = 4_usize;

        // Multiply each 4-byte word (u32) of Work by the factor, propagating carry
        // Work is processed in little-endian order (from least significant byte to most significant byte),
        // but result is stored in big-endian
        let by_chunks: Vec<u32> = work_bytes
            .chunks_exact(word_size)
            .map(|chunk| {
                let mut array = [0u8; 4];
                array.copy_from_slice(chunk);
                u32::from_le_bytes(array)
            })
            .collect();

        for (word_index, word) in by_chunks.iter().enumerate() {
            // Multiply the word by factor and add carry from previous step
            // Use u64 to avoid overflow during multiplication
            let product: u64 = (*word as u64) * (factor as u64) + carry_high;
            carry_high = product >> 32;

            // Store the low 32 bits of the product in the result
            // Result is built in big-endian order, so calculate the index accordingly
            let byte_index = by_chunks.len() - word_index;
            result_bytes[(byte_index - 1) * word_size..byte_index * word_size]
                .copy_from_slice(&(product as u32).to_be_bytes());
        }

        if carry_high > 0 {
            return Err(ChainWorkOverflow);
        }

        Ok(Work::from_be_bytes(result_bytes))
    }

    fn to_string_hex(&self) -> String {
        serialize_hex(&self.to_be_bytes())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use bitcoin::block::Header;
    use bitcoin::consensus::encode::deserialize_hex;
    use bitcoin::hashes::sha256::Hash as Sha256Hash;
    use bitcoin::params::Params;
    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::OutPoint;
    use bitcoin::Transaction;
    use bitcoin::Txid;
    use rustreexo::accumulator::proof::Proof;
    use rustreexo::accumulator::stump::Stump;

    use super::*;
    use crate::BlockConsumer;
    use crate::BlockchainError;
    use crate::UtxoData;

    #[derive(Debug)]
    pub enum MockBlockchainError {
        NotFound,
    }

    impl std::fmt::Display for MockBlockchainError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "MockBlockchainError")
        }
    }

    impl std::error::Error for MockBlockchainError {}

    pub struct MockBlockchainInterface {
        pub headers: HashMap<BlockHash, Header>,
        pub heights: HashMap<BlockHash, u32>,
        pub chain_height: u32,
    }

    impl MockBlockchainInterface {
        pub fn new() -> Self {
            Self {
                headers: HashMap::new(),
                heights: HashMap::new(),
                chain_height: 0,
            }
        }

        pub fn add_block(&mut self, hash: BlockHash, header: Header, height: u32) {
            self.headers.insert(hash, header);
            self.heights.insert(hash, height);
            self.chain_height = self.chain_height.max(height);
        }
    }

    impl BlockchainInterface for MockBlockchainInterface {
        type Error = MockBlockchainError;

        fn get_block_header(&self, hash: &BlockHash) -> Result<Header, Self::Error> {
            self.headers
                .get(hash)
                .cloned()
                .ok_or(MockBlockchainError::NotFound)
        }

        fn get_block_hash(&self, height: u32) -> Result<BlockHash, Self::Error> {
            self.heights
                .iter()
                .find(|(_, &h)| h == height)
                .map(|(hash, _)| *hash)
                .ok_or(MockBlockchainError::NotFound)
        }

        fn get_block_height(&self, hash: &BlockHash) -> Result<Option<u32>, Self::Error> {
            Ok(self.heights.get(hash).cloned())
        }

        fn get_height(&self) -> Result<u32, Self::Error> {
            Ok(self.chain_height)
        }

        fn get_tx(&self, _: &Txid) -> Result<Option<Transaction>, Self::Error> {
            unimplemented!()
        }

        fn broadcast(&self, _: &Transaction) -> Result<(), Self::Error> {
            unimplemented!()
        }

        fn estimate_fee(&self, _: usize) -> Result<f64, Self::Error> {
            unimplemented!()
        }

        fn get_block(&self, _: &BlockHash) -> Result<Block, Self::Error> {
            unimplemented!()
        }

        fn get_best_block(&self) -> Result<(u32, BlockHash), Self::Error> {
            unimplemented!()
        }

        fn subscribe(&self, _: Arc<dyn BlockConsumer>) {
            unimplemented!()
        }

        fn is_in_ibd(&self) -> bool {
            unimplemented!()
        }

        fn get_unbroadcasted(&self) -> Vec<Transaction> {
            unimplemented!()
        }

        fn is_coinbase_mature(&self, _: u32, _: BlockHash) -> Result<bool, Self::Error> {
            unimplemented!()
        }

        fn get_block_locator(&self) -> Result<Vec<BlockHash>, Self::Error> {
            unimplemented!()
        }

        fn get_block_locator_for_tip(
            &self,
            _: BlockHash,
        ) -> Result<Vec<BlockHash>, BlockchainError> {
            unimplemented!()
        }

        fn get_validation_index(&self) -> Result<u32, Self::Error> {
            unimplemented!()
        }

        fn update_acc(
            &self,
            _: Stump,
            _: &Block,
            _: u32,
            _: Proof,
            _: Vec<Sha256Hash>,
        ) -> Result<Stump, Self::Error> {
            unimplemented!()
        }

        fn get_chain_tips(&self) -> Result<Vec<BlockHash>, Self::Error> {
            unimplemented!()
        }

        fn validate_block(
            &self,
            _: &Block,
            _: Proof,
            _: HashMap<OutPoint, UtxoData>,
            _: Vec<Sha256Hash>,
            _: Stump,
        ) -> Result<(), Self::Error> {
            unimplemented!()
        }

        fn get_fork_point(&self, _: BlockHash) -> Result<BlockHash, Self::Error> {
            unimplemented!()
        }

        fn get_params(&self) -> Params {
            unimplemented!()
        }

        fn acc(&self) -> Stump {
            unimplemented!()
        }
    }

    fn get_genesis_header() -> Header {
        let genesis_header = "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c";
        let header: Header = deserialize_hex(genesis_header).expect("Failed to deserialize header");
        header
    }

    fn get_chain_and_headers(height: u32) -> (MockBlockchainInterface, Vec<Header>) {
        let mut mock_chain = MockBlockchainInterface::new();

        let mut headers = vec![];
        let mut prev_blockhash = get_genesis_header().block_hash();
        let genesis_header = get_genesis_header();
        mock_chain.add_block(prev_blockhash, genesis_header, 0);
        headers.push(genesis_header);

        for i in 1..height {
            let header = Header {
                time: 1231006505 + i * 600,
                prev_blockhash,
                ..genesis_header
            };
            headers.push(header);
            let hash = header.block_hash();
            mock_chain.add_block(hash, header, i);
            prev_blockhash = header.block_hash();
        }

        (mock_chain, headers)
    }

    #[test]
    fn test_calculate_median_time_past_more_than_11_blocks() {
        let (mock_chain, headers) = get_chain_and_headers(21);

        let median_header = headers[headers.len() - 1];
        let mtp = median_header
            .calculate_median_time_past(&mock_chain)
            .expect("Failed to calculate MTP");

        let mut times = headers
            .iter()
            .rev()
            .take(11)
            .map(|h| h.time)
            .collect::<Vec<_>>();
        times.sort();
        let expected_mtp = times[times.len() / 2];

        assert_eq!(mtp, expected_mtp);
    }

    #[test]
    fn test_calculate_median_time_past_less_than_11_blocks() {
        let (mock_chain, headers) = get_chain_and_headers(7);

        let median_header = headers[headers.len() - 1];
        let mtp = median_header
            .calculate_median_time_past(&mock_chain)
            .expect("Failed to calculate MTP");

        let mut times = headers.iter().map(|h| h.time).collect::<Vec<_>>();
        times.sort();
        let expected_mtp = times[times.len() / 2];

        assert_eq!(mtp, expected_mtp);
    }

    #[test]
    fn test_calculate_median_time_past_genesis_only() {
        let (mock_chain, headers) = get_chain_and_headers(1);

        // Test the MTP calculation
        let median_header = headers[0];
        let mtp = median_header
            .calculate_median_time_past(&mock_chain)
            .expect("Failed to calculate MTP");

        let expected_mtp = headers[0].time;

        assert_eq!(mtp, expected_mtp);
    }

    #[test]
    fn test_calculate_chain_work() {
        let (mock_chain, headers) = get_chain_and_headers(3000);
        let header = headers[headers.len() - 1];

        let work = header
            .calculate_chain_work(&mock_chain)
            .expect("Failed to calculate chain work");

        let expected_hex_string =
            "00000000000000000000000000000000000000000000000000000bb80bb80bb8";
        let expected_work = Work::from_hex(&format!("0x{expected_hex_string}")).unwrap();

        assert_eq!(work.to_string_hex(), expected_hex_string);
        assert_eq!(work, expected_work);
    }

    #[test]
    fn test_get_next_block_hash() {
        let (mock_chain, headers) = get_chain_and_headers(5);

        let header = headers[2];
        let next_hash = header
            .get_next_block_hash(&mock_chain)
            .expect("Failed to get next block hash")
            .expect("Next block hash is None");

        let expected_hash = headers[3].block_hash();

        assert_eq!(next_hash, expected_hash);

        let last_header = headers[headers.len() - 1];
        let next_hash = last_header
            .get_next_block_hash(&mock_chain)
            .expect("Failed to get next block hash");

        assert!(next_hash.is_none());
    }

    #[test]
    fn test_get_bits() {
        let header = get_genesis_header();
        let bits_hex = header.get_bits_hex();
        assert_eq!(bits_hex, "1d00ffff");
    }

    #[test]
    fn test_get_confirmations() {
        let (mock_chain, headers) = get_chain_and_headers(5);

        let header = headers[2];
        let confirmations = header
            .get_confirmations(&mock_chain)
            .expect("Failed to get confirmations");

        let expected_confirmations = headers.len() - 2;

        assert_eq!(confirmations, expected_confirmations as u32);
    }

    #[test]
    fn test_get_difficulty() {
        let header = get_genesis_header();
        let difficulty = header.get_difficulty();
        assert_eq!(difficulty, 1.0);
    }

    #[test]
    fn test_get_height() {
        let (mock_chain, headers) = get_chain_and_headers(5);
        let height_expected = 3;

        let header = headers[height_expected];
        let height = header
            .get_height(&mock_chain)
            .expect("Failed to get block height");

        assert_eq!(height, height_expected as u32);

        let mut header_missing = headers[0];
        header_missing.nonce = 0;
        let result = header_missing.get_height(&mock_chain);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_target() {
        let header = get_genesis_header();
        let target_hex = header.get_target_hex();
        assert_eq!(
            target_hex,
            "00000000ffff0000000000000000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn test_get_version_hex() {
        let header = get_genesis_header();
        let version_hex = header.get_version_hex();
        assert_eq!(version_hex, "00000001");
    }

    #[test]
    fn test_multiply_work_by_u32_success() {
        let work_bytes: [u8; 32] = [
            0, 0, 0, 3, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0,
            0, 0, 4,
        ];
        let work = Work::from_be_bytes(work_bytes);
        let factor = 2;

        let result = work.multiply_work_by_u32(factor).unwrap();

        let expected_bytes: [u8; 32] = [
            0, 0, 0, 6, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 0, 0,
            0, 0, 8,
        ];
        let expected = Work::from_be_bytes(expected_bytes);

        assert_eq!(result, expected);
    }

    #[test]
    fn test_multiply_work_by_u32_overflow() {
        let work_bytes: [u8; 32] = [
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF,
        ];
        let work = Work::from_be_bytes(work_bytes);
        let factor = u32::MAX;

        let result = work.multiply_work_by_u32(factor);

        assert_eq!(result, Err(ChainWorkOverflow));
    }
}
