// SPDX-License-Identifier: MIT OR Apache-2.0

use core::error::Error;
use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;

use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::Network;
use bitcoin::Work;
use bitcoin::block::Header;
use bitcoin::consensus::encode::serialize_hex;
use bitcoin::p2p::ServiceFlags;
use floresta_common::bhash;
use floresta_common::prelude::Box;
use floresta_common::prelude::HashSet;
use floresta_common::prelude::String;
use floresta_common::prelude::ToString;
use floresta_common::prelude::Vec;
use floresta_common::service_flags;

use crate::BlockchainInterface;

const MEDIAN_TIME_PAST_BLOCK_COUNT: usize = 11;

pub trait Bip30UnspendableExt {
    /// Returns true if the coinbase output in this block is BIP-30 unspendable.
    fn is_bip30_unspendable(&self, height: u32) -> bool;
}

impl Bip30UnspendableExt for Block {
    fn is_bip30_unspendable(&self, height: u32) -> bool {
        let bhash_91722 =
            bhash!("00000000000271a2dc26e7667f8419f2e15416dc6955e5a6c6cdf3f2574dd08e");
        let bhash_91812 =
            bhash!("00000000000af0aed4792b1acee3d966af36cf5def14935db8de83d6f9306f2f");

        match height {
            91722 => self.block_hash() == bhash_91722,
            91812 => self.block_hash() == bhash_91812,
            _ => false,
        }
    }
}

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

impl Display for HeaderExtError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Chain(e) => write!(f, "Chain error: {e}"),
            Self::BlockNotFound => write!(f, "Block not found"),
            Self::ChainWorkOverflow => write!(f, "Chain work overflow"),
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
        chain
            .get_work(self.block_hash())
            .map_err(|err| HeaderExtError::Chain(Box::new(err)))
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
            return Ok(Self::from_be_bytes([0u8; 32]));
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

        Ok(Self::from_be_bytes(result_bytes))
    }

    fn to_string_hex(&self) -> String {
        serialize_hex(&self.to_be_bytes())
    }
}

/// A dns seed is a authoritative DNS server that returns the IP addresses of nodes that are
/// likely to be accepting incoming connections. This is our preferred way of finding new peers
/// on the first startup, as peers returned by seeds are likely to be online and accepting
/// connections. We may use this as a fallback if we don't have any peers to connect in
/// subsequent startups.
///
/// Some seeds allow filtering by service flags, so we may use this to find peers that are
/// likely to be running Utreexo, for example.
pub struct DnsSeed {
    /// The domain name of the seed
    pub seed: &'static str,

    /// Useful filters we can use to find relevant peers
    pub filters: ServiceFlags,
}

/// This functionality is used to create a new DNS seed with possible filters.
impl DnsSeed {
    /// Create a new DNS seed
    pub fn new(seed: &'static str, filters: ServiceFlags) -> Self {
        Self { seed, filters }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuriedDeployment {
    Bip34,
    Bip65,
    Bip66,
    Csv,
    Segwit,
}

impl BuriedDeployment {
    /// Name used in the `getdeploymentinfo` RPC response.
    pub fn to_deployment_name(self) -> &'static str {
        match self {
            Self::Bip34 => "bip34",
            Self::Bip65 => "bip65",
            Self::Bip66 => "bip66",
            Self::Csv => "csv",
            Self::Segwit => "segwit",
        }
    }

    /// Script flags enabled by this deployment. Empty for deployments
    /// that do not change script validation (BIP34 only changes coinbase structure).
    pub fn to_script_flags(self) -> &'static [&'static str] {
        match self {
            Self::Bip34 => &[],
            Self::Bip65 => &["CHECKLOCKTIMEVERIFY"],
            Self::Bip66 => &["DERSIG"],
            Self::Csv => &["CHECKSEQUENCEVERIFY"],
            Self::Segwit => &["WITNESS", "NULLDUMMY"],
        }
    }
}

pub trait NetworkExt {
    /// Get a list of [`DnsSeed`]s for a given [`Network`].
    ///
    /// Some DNS seeds allow requesting addresses using a [`ServiceFlags`] filter.
    /// Here we define `x9`, `x49`, and `x1009` (the relevant services for this node
    /// to operate), and use them to request addresses from the DNS seeds that support it.
    fn get_chain_dns_seeds(&self) -> Vec<DnsSeed>;

    /// Returns the buried deployment list for a network, as `(name, activation_height)` pairs.
    ///
    /// Heights are sourced from Bitcoin Core's `chainparams.cpp` at v30.2
    /// (commit `4d7d5f6b79d4c11c47e7a828d81296918fd11d4d`):
    /// <https://github.com/bitcoin/bitcoin/blob/4d7d5f6b79d4c11c47e7a828d81296918fd11d4d/src/kernel/chainparams.cpp>
    //
    // TODO: also emit BIP9 deployments (`taproot`, `testdummy`); requires the versionbits state machine.
    fn buried_deployments_for(&self) -> &'static [(BuriedDeployment, u32)];

    /// Returns the set of script-validation flags active for this network at the given block hash
    /// and height, including historical exception blocks.
    fn get_script_flags(&self, hash: BlockHash, height: u32) -> HashSet<String>;
}

impl NetworkExt for Network {
    fn get_chain_dns_seeds(&self) -> Vec<DnsSeed> {
        let mut seeds = Vec::new();

        let none = ServiceFlags::NONE;
        let x9 = ServiceFlags::NETWORK | ServiceFlags::WITNESS;
        let x49 = ServiceFlags::NETWORK | ServiceFlags::WITNESS | ServiceFlags::COMPACT_FILTERS;
        let x1009 = ServiceFlags::NETWORK | ServiceFlags::WITNESS | service_flags::UTREEXO.into();
        let x1000 = service_flags::UTREEXO.into();

        #[rustfmt::skip]
        match self {
            Self::Bitcoin => {
                seeds.push(DnsSeed::new("seed.calvinkim.info", x1009));
                seeds.push(DnsSeed::new("seed.bitcoin.luisschwab.com", x1009));
                seeds.push(DnsSeed::new("seed.bitcoin.sipa.be", x9));
                seeds.push(DnsSeed::new("dnsseed.bluematt.me", x49));
                seeds.push(DnsSeed::new("seed.bitcoinstats.com", x49));
                seeds.push(DnsSeed::new("seed.btc.petertodd.org", x49));
                seeds.push(DnsSeed::new("seed.bitcoin.sprovoost.nl", x49));
                seeds.push(DnsSeed::new("dnsseed.emzy.de", x49));
                seeds.push(DnsSeed::new("seed.bitcoin.wiz.biz", x49));
                seeds.push(DnsSeed::new("bitcoin.seed.dlsouza.lol", x1000));
            }
            Self::Signet => {
                seeds.push(DnsSeed::new("signet.seed.dlsouza.lol", x1000));
                seeds.push(DnsSeed::new("seed.signet.bitcoin.sprovoost.nl", x49));
            }
            Self::Testnet => {
                seeds.push(DnsSeed::new("testnet-seed.bitcoin.jonasschnelli.ch", x49));
                seeds.push(DnsSeed::new("testnet.seed.dlsouza.lol", x1000));
                seeds.push(DnsSeed::new("seed.tbtc.petertodd.org", x49));
                seeds.push(DnsSeed::new("seed.testnet.bitcoin.sprovoost.nl", x49));
                seeds.push(DnsSeed::new("testnet-seed.bluematt.me", none));
            }
            Self::Testnet4 => {
                seeds.push(DnsSeed::new("seed.testnet4.bitcoin.sprovoost.nl", none));
                seeds.push(DnsSeed::new("seed.testnet4.wiz.biz", none));
            }
            Self::Regtest => {}
        };

        seeds
    }

    fn buried_deployments_for(&self) -> &'static [(BuriedDeployment, u32)] {
        const MAINNET_BURIED: &[(BuriedDeployment, u32)] = &[
            (BuriedDeployment::Bip34, 227_931),
            (BuriedDeployment::Bip66, 363_725),
            (BuriedDeployment::Bip65, 388_381),
            (BuriedDeployment::Csv, 419_328),
            (BuriedDeployment::Segwit, 481_824),
        ];

        const TESTNET3_BURIED: &[(BuriedDeployment, u32)] = &[
            (BuriedDeployment::Bip34, 21_111),
            (BuriedDeployment::Bip66, 330_776),
            (BuriedDeployment::Bip65, 581_885),
            (BuriedDeployment::Csv, 770_112),
            (BuriedDeployment::Segwit, 834_624),
        ];

        const TESTNET4_BURIED: &[(BuriedDeployment, u32)] = &[
            (BuriedDeployment::Bip34, 1),
            (BuriedDeployment::Bip66, 1),
            (BuriedDeployment::Bip65, 1),
            (BuriedDeployment::Csv, 1),
            (BuriedDeployment::Segwit, 1),
        ];

        const SIGNET_BURIED: &[(BuriedDeployment, u32)] = &[
            (BuriedDeployment::Bip34, 1),
            (BuriedDeployment::Bip66, 1),
            (BuriedDeployment::Bip65, 1),
            (BuriedDeployment::Csv, 1),
            (BuriedDeployment::Segwit, 1),
        ];

        const REGTEST_BURIED: &[(BuriedDeployment, u32)] = &[
            (BuriedDeployment::Bip34, 1),
            (BuriedDeployment::Bip66, 1),
            (BuriedDeployment::Bip65, 1),
            (BuriedDeployment::Csv, 1),
            (BuriedDeployment::Segwit, 0),
        ];

        match self {
            Self::Bitcoin => MAINNET_BURIED,
            Self::Testnet => TESTNET3_BURIED,
            Self::Testnet4 => TESTNET4_BURIED,
            Self::Signet => SIGNET_BURIED,
            Self::Regtest => REGTEST_BURIED,
        }
    }

    fn get_script_flags(&self, hash: BlockHash, height: u32) -> HashSet<String> {
        //Helper function to create a `HashSet` from an array of str.
        fn flags_set(flags: &[&str]) -> HashSet<String> {
            flags.iter().map(|f| (*f).to_string()).collect()
        }

        // BIP16 didn't become active until Apr 1 2012 (on mainnet, and retroactively applied to testnet)
        // However, only one historical block violated the P2SH rules (on both mainnet and testnet).
        // Similarly, only one historical block violated the TAPROOT rules on mainnet.
        // For simplicity, always leave P2SH+WITNESS+TAPROOT on except for the two violating blocks.
        let mut flags = match (self, hash) {
            (Self::Bitcoin, h)
                if h == bhash!(
                    "00000000000002dc756eebf4f49723ed8d30cc28a5f108eb94b1ba88ac4f9c22"
                ) =>
            {
                HashSet::new()
            }
            (Self::Bitcoin, h)
                if h == bhash!(
                    "0000000000000000000f14c35b2d841e986ab5441de8c585d5ffe55ea1e395ad"
                ) =>
            {
                flags_set(&["P2SH", "WITNESS"])
            }
            (Self::Testnet, h)
                if h == bhash!(
                    "00000000dd30457c001f4095d208cc1296b0eed002427aa599874af7a432b105"
                ) =>
            {
                HashSet::new()
            }
            // Only mainnet and testnet3 require additional validation. The other networks do not.
            _ => flags_set(&["P2SH", "WITNESS", "TAPROOT"]),
        };

        for (deployment, activation_height) in self.buried_deployments_for() {
            if height + 1 >= *activation_height {
                flags.extend(
                    deployment
                        .to_script_flags()
                        .iter()
                        .map(|f| (*f).to_string()),
                );
            }
        }

        flags
    }
}

#[cfg(test)]
mod tests {
    use core::fmt;
    use core::fmt::Display;
    use core::fmt::Formatter;
    use std::collections::HashMap;
    use std::sync::Arc;

    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::OutPoint;
    use bitcoin::Transaction;
    use bitcoin::Txid;
    use bitcoin::block::Header;
    use bitcoin::consensus::encode::deserialize_hex;
    use bitcoin::hashes::sha256::Hash as Sha256Hash;
    use bitcoin::params::Params;
    use rustreexo::proof::Proof;
    use rustreexo::stump::Stump;

    use super::*;
    use crate::BlockConsumer;
    use crate::BlockchainError;
    use crate::UtxoData;
    use crate::pruned_utreexo::IBDState;

    #[derive(Debug)]
    pub enum MockBlockchainError {
        NotFound,
    }

    impl Display for MockBlockchainError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(f, "MockBlockchainError")
        }
    }

    impl core::error::Error for MockBlockchainError {}

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

        fn size_on_disk(&self) -> Result<u64, Self::Error> {
            unimplemented!("MockBlockchainInterface has no on-disk presence")
        }

        fn get_block_header(&self, hash: &BlockHash) -> Result<Header, Self::Error> {
            self.headers
                .get(hash)
                .cloned()
                .ok_or(MockBlockchainError::NotFound)
        }

        fn get_block_hash(&self, height: u32) -> Result<BlockHash, Self::Error> {
            self.heights
                .iter()
                .find(|(_, h)| **h == height)
                .map(|(hash, _)| *hash)
                .ok_or(MockBlockchainError::NotFound)
        }

        fn get_block_height(&self, hash: &BlockHash) -> Result<Option<u32>, Self::Error> {
            Ok(self.heights.get(hash).cloned())
        }

        fn get_height(&self) -> Result<u32, Self::Error> {
            Ok(self.chain_height)
        }

        fn get_work(&self, _tip: BlockHash) -> Result<Work, Self::Error> {
            let work_hex = "00000000000000000000000000000000000000000000000000000bb80bb80bb8";
            Ok(Work::from_hex(&format!("0x{work_hex}")).expect("hardcoded work"))
        }

        fn get_tx(&self, _: &Txid) -> Result<Option<Transaction>, Self::Error> {
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

        fn ibd_state(&self) -> IBDState {
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
}
