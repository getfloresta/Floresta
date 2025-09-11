//! This module provides configuration and parameters for different Bitcoin networks (mainnet,
//! testnet, signet, and regtest).
//!
//! It includes:
//! - Network-specific parameters like block reward halving intervals and maturity periods
//! - DNS seeds for peer discovery
//! - Assumable validation states for Utreexo
//! - Block verification flag exceptions
//!
//! The main struct [`ChainParams`] encapsulates all chain-specific parameters while
//! [`DnsSeed`] handles peer discovery through DNS.

extern crate alloc;
use alloc::vec::Vec;
use core::ffi::c_uint;

use bitcoin::blockdata::constants::genesis_block;
use bitcoin::p2p::ServiceFlags;
use bitcoin::params::Params;
use bitcoin::Block;
use bitcoin::BlockHash;
use bitcoin::Network;
use floresta_common::acchashes;
use floresta_common::bhash;
use rustreexo::accumulator::node_hash::BitcoinNodeHash;

use crate::prelude::*;
use crate::AssumeValidArg;

#[derive(Clone, Debug)]
/// This struct encapsulates all chain-specific parameters.
pub struct ChainParams {
    /// Field to store parameters related to the chain consensus.
    pub params: Params,

    /// The network's first block, also called genesis block.
    pub genesis: Block,

    /// Interval of blocks until the block reward halves
    pub subsidy_halving_interval: u64,

    /// When we retarget we expect this many seconds to be elapsed since last time. If
    /// it's more, we decrease difficulty, if it's less we increase difficulty
    pub pow_target_timespan: u64,

    /// We wait this many blocks before a coinbase output can be spent
    pub coinbase_maturity: u32,

    /// The height at which segwit is activated
    pub segwit_activation_height: u32,

    /// The height at which csv(CHECK_SEQUENCE_VERIFY) is activated
    pub csv_activation_height: u32,

    /// A list of exceptions to the rules, where the key is the block hash and the value is the
    /// verification flags
    pub exceptions: HashMap<BlockHash, c_uint>,

    /// The network this chain params is for
    pub network: Network,

    /// Whether we should enforce BIP-094 "Testnet 4" rules
    pub enforce_bip94: bool,
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
    /// The network this peer supports (e.g, mainnet, testnet, etc)
    pub network: Network,

    /// The domain name of the seed
    pub seed: &'static str,

    /// Useful filters we can use to find relevant peers
    pub filters: ServiceFlags,
}

/// This functionality is used to create a new DNS seed with possible filters.
impl DnsSeed {
    /// Create a new DNS seed
    pub fn new(network: Network, seed: &'static str, filters: ServiceFlags) -> Self {
        DnsSeed {
            network,
            seed,
            filters,
        }
    }
}

/// If enabled, the node will assume that the provided Utreexo state is valid, and will
/// start running from there. You may use this to make your node start faster, but you
/// should be sure that the provided state is valid. You may or not verify the state,
/// by downloading all blocks on background, and then verifying the final Utreexo state.
#[derive(Debug, Clone)]
pub struct AssumeUtreexoValue {
    /// The latest block assumed to be valid. This acc is the roots at this block
    pub block_hash: BlockHash,

    /// Same as block_hash, but in height
    pub height: u32,

    /// The roots of the Utreexo accumulator at this block
    pub roots: Vec<BitcoinNodeHash>,

    /// The number of leaves in the Utreexo accumulator at this block
    pub leaves: u64,

    /// Optional: the filter header for the block filter at the assumeutreexo block
    ///
    /// We use this value to verify that the filters we get from our peers are correct,
    /// without this, they can give us incorrect filters and we won't be able to detect it.
    pub filter_header: Option<BlockHash>,
}

impl ChainParams {
    /// This method is called when Assume Utreexo is set to true. It means that the user will accept the hardcoded utreexo state for the specified block, if it is found in the best chain. We can then sync rapidly from this state.
    pub fn get_assume_utreexo(network: Network) -> AssumeUtreexoValue {
        let genesis = genesis_block(Params::new(network));
        match network {
            Network::Bitcoin => AssumeUtreexoValue {
                block_hash: bhash!(
                    "0000000000000000000239f2b7f982df299193bdd693f499e6b893d8276ab7ce"
                ),
                height: 902967,
                roots: acchashes![
                    "bd53eef66849c9d3ca13b62ce694030ac4d4b484c6f490f473b9868a7c5df2e8",
                    "993ffb1782db628c18c75a5edb58d4d506167d85ca52273e108f35b73bb5b640",
                    "36d8c4ba5176c816bdae7c4119d9f2ea26a1f743f5e6e626102f66a835eaac6d",
                    "4c93092c1ecd843d2b439365609e7f616fe681de921a46642951cb90873ba6ce",
                    "9b4435987e18e1fe4efcb6874bba5cdc66c3e3c68229f54624cb6343787488a4",
                    "ab1e87c4066bf195fa7b8357874b82de4fa09ddba921499d6fc73aa133200505",
                    "8f8215e284dbce604988755ba3c764dbfa024ae0d9659cd67b24742f46360849",
                    "09b5057a8d6e1f61e93baf474220f581bd1a38d8a378dacb5f7fdec532f21e00",
                    "a331072d7015c8d33a5c17391264a72a7ca1c07e1f5510797064fced7fbe591d",
                    "c1c647289156980996d9ea46377e8c1b7e5c05940730ef8c25c0d081341221b5",
                    "330115a495ed14140cd785d44418d84b872480d293972abd66e3325fdc78ac93",
                    "b1d7a488e1197908efb2091a3b750508cb2fc495d2011bf2c34c5ae2d40bd2a5",
                    "3b3b2e51ad96e1ae8ce468c7947b8aa2b41ecb400a32edec3dbcfe5ddb9aca50",
                    "9d852775775f4c1e4a150404776a6b22569a0fe31f2e669fd3b31a0f70072800",
                    "8e5f6a92169ad67b3f2682f230e2a62fc849b0a47bc36af8ce6cae24a5343126",
                    "6dbd2925f8aa0745ac34fc9240ce2a7ef86953fc305c6570ef580a0763072bbe",
                    "8121c38dcb37684c6d50175f5fd2695af3b12ce0263d20eb7cc503b96f7dba0d",
                    "f5d8b30dd2038e1b3a5ced7a30c961e230270020c336fb649d0a9e169f11b876",
                    "0466bd4eb9e7be5b8870e97d2a66377525391c16f15dbcc3833853c8d3bae51e",
                    "976184c55f74cbb780938a20e2a5df2791cf51e712f68a400a6b024c77ad78e4",
                ]
                .to_vec(),
                leaves: 2860457445,
                filter_header: None,
            },
            Network::Testnet => AssumeUtreexoValue {
                block_hash: genesis.block_hash(),
                height: 0,
                leaves: 0,
                roots: Vec::new(),
                filter_header: None,
            },
            Network::Testnet4 => AssumeUtreexoValue {
                block_hash: genesis.block_hash(),
                height: 0,
                leaves: 0,
                roots: Vec::new(),
                filter_header: None,
            },
            Network::Signet => AssumeUtreexoValue {
                block_hash: genesis.block_hash(),
                height: 0,
                leaves: 0,
                roots: Vec::new(),
                filter_header: None,
            },
            Network::Regtest => AssumeUtreexoValue {
                block_hash: genesis.block_hash(),
                height: 0,
                leaves: 0,
                roots: Vec::new(),
                filter_header: None,
            },
        }
    }

    /// This method is used to assume all the scripts up to a specific block in the chain as valid. It can be None (we will verify all the scripts), user input or hardcoded.
    pub fn get_assume_valid(network: Network, arg: AssumeValidArg) -> Option<BlockHash> {
        match arg {
            AssumeValidArg::Disabled => None,
            AssumeValidArg::UserInput(hash) => Some(hash),
            AssumeValidArg::Hardcoded => match network {
                Network::Bitcoin => Some(bhash!(
                    "00000000000000000001ff36aef3a0454cf48887edefa3aab1f91c6e67fee294"
                )),
                Network::Testnet => Some(bhash!(
                    "000000007df22db38949c61ceb3d893b26db65e8341611150e7d0a9cd46be927"
                )),
                Network::Testnet4 => Some(bhash!(
                    "0000000000335c2895f02ebc75773d2ca86095325becb51773ce5151e9bcf4e0"
                )),
                Network::Signet => Some(bhash!(
                    "000000084ece77f20a0b6a7dda9163f4527fd96d59f7941fb8452b3cec855c2e"
                )),
                Network::Regtest => Some(bhash!(
                    "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
                )),
            },
        }
    }

    #[cfg(feature = "bitcoinkernel")]
    /// Returns the validation flags for a given block hash and height
    pub fn get_validation_flags(&self, height: u32, hash: BlockHash) -> c_uint {
        if let Some(flag) = self.exceptions.get(&hash) {
            return *flag;
        }

        // From Bitcoin Core:
        // BIP16 didn't become active until Apr 1 2012 (on mainnet, and
        // retroactively applied to testnet)
        // However, only one historical block violated the P2SH rules (on both
        // mainnet and testnet).
        // Similarly, only one historical block violated the TAPROOT rules on
        // mainnet.
        // For simplicity, always leave P2SH+WITNESS+TAPROOT on except for the two
        // violating blocks.
        let mut flags = bitcoinkernel::VERIFY_P2SH
            | bitcoinkernel::VERIFY_WITNESS
            | bitcoinkernel::VERIFY_TAPROOT;

        if height >= self.params.bip65_height {
            flags |= bitcoinkernel::VERIFY_CHECKLOCKTIMEVERIFY;
        }
        if height >= self.params.bip66_height {
            flags |= bitcoinkernel::VERIFY_DERSIG;
        }
        if height >= self.csv_activation_height {
            flags |= bitcoinkernel::VERIFY_CHECKSEQUENCEVERIFY;
        }
        if height >= self.segwit_activation_height {
            flags |= bitcoinkernel::VERIFY_NULLDUMMY;
        }

        flags
    }
}

#[cfg(feature = "bitcoinkernel")]
/// There's almost no transactions in the chain that
/// "looks like segwit but are not segwit". We pretend segwit
/// was enabled since genesis, and only skip this for blocks
/// that have such transactions using hardcoded values.
fn get_exceptions() -> HashMap<BlockHash, c_uint> {
    use bitcoinkernel::VERIFY_NONE;
    use bitcoinkernel::VERIFY_P2SH;
    use bitcoinkernel::VERIFY_WITNESS;

    let mut exceptions = HashMap::new();
    exceptions.insert(
        bhash!("00000000000002dc756eebf4f49723ed8d30cc28a5f108eb94b1ba88ac4f9c22"),
        VERIFY_NONE,
    ); // BIP16 exception on main net
    exceptions.insert(
        bhash!("0000000000000000000f14c35b2d841e986ab5441de8c585d5ffe55ea1e395ad"),
        VERIFY_P2SH | VERIFY_WITNESS,
    ); // Taproot exception on main net
    exceptions.insert(
        bhash!("00000000dd30457c001f4095d208cc1296b0eed002427aa599874af7a432b105"),
        VERIFY_NONE,
    ); // BIP16 exception on test net
    exceptions
}

#[cfg(not(feature = "bitcoinkernel"))]
fn get_exceptions() -> HashMap<BlockHash, c_uint> {
    HashMap::new()
}

impl AsRef<Params> for ChainParams {
    fn as_ref(&self) -> &Params {
        &self.params
    }
}

impl From<Network> for ChainParams {
    fn from(network: Network) -> Self {
        let genesis = genesis_block(Params::new(network));
        let exceptions = get_exceptions();

        match network {
            Network::Bitcoin => ChainParams {
                params: Params::new(network),
                network,
                genesis,
                pow_target_timespan: 14 * 24 * 60 * 60, // two weeks
                subsidy_halving_interval: 210_000,
                coinbase_maturity: 100,
                segwit_activation_height: 481_824,
                csv_activation_height: 419_328,
                exceptions,
                enforce_bip94: false,
            },
            Network::Testnet => ChainParams {
                params: Params::new(network),
                network,
                genesis,
                pow_target_timespan: 14 * 24 * 60 * 60, // two weeks
                subsidy_halving_interval: 210_000,
                coinbase_maturity: 100,
                segwit_activation_height: 834_624,
                csv_activation_height: 770_112,
                exceptions,
                enforce_bip94: false,
            },
            Network::Testnet4 => ChainParams {
                params: Params::new(network),
                network,
                genesis,
                pow_target_timespan: 14 * 24 * 60 * 60,
                subsidy_halving_interval: 210_000,
                coinbase_maturity: 100,
                segwit_activation_height: 1,
                csv_activation_height: 1,
                exceptions,
                enforce_bip94: true,
            },
            Network::Signet => ChainParams {
                params: Params::new(network),
                network,
                genesis,
                pow_target_timespan: 14 * 24 * 60 * 60, // two weeks
                subsidy_halving_interval: 210_000,
                coinbase_maturity: 100,
                csv_activation_height: 1,
                segwit_activation_height: 1,
                exceptions,
                enforce_bip94: false,
            },
            Network::Regtest => ChainParams {
                params: Params::new(network),
                network,
                genesis,
                pow_target_timespan: 14 * 24 * 60 * 60, // two weeks
                subsidy_halving_interval: 150,
                coinbase_maturity: 100,
                csv_activation_height: 0,
                segwit_activation_height: 0,
                exceptions,
                enforce_bip94: false,
            },
        }
    }
}

/// This function returns the DNS seeds for the given network.
///
/// DNS seeds taken from Bitcoin Core at commit 382b692a503355df7347efd9c128aff465b5583e
///
/// Some dns seeds lets us filter the returned peers by advertised services. We are interested
/// in peers with: UTREEXO, COMPACT_FILTERS, WITNESS and NETWORK. Not all seeds supports all
/// bits, so from this list, we pick the ones they support, and ask for this.
pub fn get_chain_dns_seeds(network: Network) -> Vec<DnsSeed> {
    let mut seeds = Vec::new();

    // x9 or 0x09 means NETWORK + WITNESS
    let x9: ServiceFlags = ServiceFlags::from(0x9);
    // this mean NETWORK + WITNESS + COMPACT_FILTERS
    let x49 = ServiceFlags::from(0x49);
    // this mean NETWORK + WITNESS + COMPACT_FILTERS + UTREEXO
    let x1000049 = ServiceFlags::from(0x1000049);
    // this means NETWORK + WITNESS + UTREEXO
    let x1000009 = ServiceFlags::from(0x1000009);
    // filters aren't supported (usually returns a static list of peers)
    let none = ServiceFlags::NONE;

    match network {
        Network::Bitcoin => {
            seeds.push(DnsSeed::new(
                Network::Bitcoin,
                "seed.calvinkim.info",
                x1000009,
            ));
            seeds.push(DnsSeed::new(
                Network::Bitcoin,
                "seed.bitcoin.luisschwab.com",
                x1000049,
            ));
            seeds.push(DnsSeed::new(
                Network::Bitcoin,
                "seed.bitcoin.sipa.be",
                x9, // no COMPACT_FILTERS
            ));
            seeds.push(DnsSeed::new(Network::Bitcoin, "dnsseed.bluematt.me", x49));
            seeds.push(DnsSeed::new(Network::Bitcoin, "seed.bitcoinstats.com", x49));
            seeds.push(DnsSeed::new(
                Network::Bitcoin,
                "seed.btc.petertodd.org",
                x49,
            ));
            seeds.push(DnsSeed::new(
                Network::Bitcoin,
                "seed.bitcoin.sprovoost.nl",
                x49,
            ));
            seeds.push(DnsSeed::new(Network::Bitcoin, "dnsseed.emzy.de", x49));
            seeds.push(DnsSeed::new(Network::Bitcoin, "seed.bitcoin.wiz.biz", x49));
        }
        Network::Testnet => {
            seeds.push(DnsSeed::new(
                Network::Testnet,
                "testnet-seed.bitcoin.jonasschnelli.ch",
                x49,
            ));
            seeds.push(DnsSeed::new(
                Network::Testnet,
                "seed.tbtc.petertodd.org",
                x49,
            ));
            seeds.push(DnsSeed::new(
                Network::Testnet,
                "seed.testnet.bitcoin.sprovoost.nl",
                x49,
            ));
            seeds.push(DnsSeed::new(
                Network::Testnet,
                "testnet-seed.bluematt.me",
                none,
            ));
        }
        Network::Testnet4 => {
            seeds.push(DnsSeed::new(
                Network::Testnet4,
                "seed.testnet4.bitcoin.sprovoost.nl",
                none,
            ));
            seeds.push(DnsSeed::new(
                Network::Testnet4,
                "seed.testnet4.wiz.biz",
                none,
            ));
        }
        Network::Signet => {
            seeds.push(DnsSeed::new(
                Network::Signet,
                "seed.signet.bitcoin.sprovoost.nl",
                x49,
            ));
            seeds.push(DnsSeed::new(Network::Signet, "seed.dlsouza.lol", x1000049));
        }
        Network::Regtest => {
            // No seeds for regtest
        }
    };

    seeds
}
