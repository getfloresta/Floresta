use std::fmt::Debug;
mod parsers;

use anyhow::Ok;
use bitcoin::BlockHash;
use bitcoin::Network;
use bitcoin::Txid;
use clap::Parser;
use clap::Subcommand;
use floresta_rpc::jsonrpc_client::Client;
use floresta_rpc::rpc::FlorestaRPC;
use floresta_rpc::rpc_types::AddNodeCommand;
use floresta_rpc::rpc_types::RescanConfidence;

// Main function that runs the CLI application
fn main() -> anyhow::Result<()> {
    // Parse command line arguments into a Cli struct
    let cli = Cli::parse();

    // Create a new JSON-RPC client using the host from the CLI arguments
    let client = Client::new(get_host(&cli));

    // Perform the requested RPC call and get the result
    let res = do_request(&cli, client)?;

    // Print the result to the console
    println!("{res}");

    // Return Ok to indicate the program ran successfully
    anyhow::Ok(())
}

// Function to determine the RPC host based on CLI arguments and network type
fn get_host(cmd: &Cli) -> String {
    // If a specific RPC host is provided, use it
    if let Some(host) = cmd.rpc_host.clone() {
        return host;
    }

    // Otherwise, use the default host based on the network type
    //
    // TODO(@luisschwab): use `NetworkExt` to append the correct port
    // once https://github.com/rust-bitcoin/rust-bitcoin/pull/4639 makes it into a release.
    match cmd.network {
        Network::Bitcoin => "http://127.0.0.1:8332".into(),
        Network::Signet => "http://127.0.0.1:38332".into(),
        Network::Testnet => "http://127.0.0.1:18332".into(),
        Network::Testnet4 => "http://127.0.0.1:48332".into(),
        Network::Regtest => "http://127.0.0.1:18442".into(),
    }
}

// Function to perform the requested RPC call based on CLI arguments
fn do_request(cmd: &Cli, client: Client) -> anyhow::Result<String> {
    Ok(match cmd.methods.clone() {
        // Handle each possible RPC method and serialize the result to a pretty JSON string
        Methods::GetBlockchainInfo => serde_json::to_string_pretty(&client.get_blockchain_info()?)?,
        Methods::GetBlockHash { height } => {
            serde_json::to_string_pretty(&client.get_block_hash(height)?)?
        }
        Methods::GetBestBlockHash => serde_json::to_string_pretty(&client.get_best_block_hash()?)?,
        Methods::GetBlockCount => serde_json::to_string_pretty(&client.get_block_count()?)?,
        Methods::GetTxOut { txid, vout } => {
            serde_json::to_string_pretty(&client.get_tx_out(txid, vout)?)?
        }
        Methods::GetTxOutProof { txids, blockhash } => {
            serde_json::to_string_pretty(&client.get_txout_proof(txids, blockhash))?
        }
        Methods::GetTransaction { txid, .. } => {
            serde_json::to_string_pretty(&client.get_transaction(txid, Some(true))?)?
        }
        Methods::RescanBlockchain {
            start_block,
            stop_block,
            use_timestamp,
            confidence,
        } => serde_json::to_string_pretty(&client.rescanblockchain(
            Some(start_block),
            Some(stop_block),
            use_timestamp,
            confidence,
        )?)?,
        Methods::SendRawTransaction { tx } => {
            serde_json::to_string_pretty(&client.send_raw_transaction(tx)?)?
        }
        Methods::GetBlockHeader { hash } => {
            serde_json::to_string_pretty(&client.get_block_header(hash)?)?
        }
        Methods::LoadDescriptor { desc } => {
            serde_json::to_string_pretty(&client.load_descriptor(desc)?)?
        }
        Methods::GetRoots => serde_json::to_string_pretty(&client.get_roots()?)?,
        Methods::GetBlock { hash, verbosity } => {
            serde_json::to_string_pretty(&client.get_block(hash, verbosity)?)?
        }
        Methods::GetPeerInfo => serde_json::to_string_pretty(&client.get_peer_info()?)?,
        Methods::Stop => serde_json::to_string_pretty(&client.stop()?)?,
        Methods::AddNode {
            node,
            command,
            v2transport,
        } => {
            let transport = v2transport.unwrap_or(false);
            serde_json::to_string_pretty(&client.add_node(node, command, transport)?)?
        }
        Methods::DisconnectNode {
            node_address,
            node_id,
        } => serde_json::to_string_pretty(&client.disconnect_node(node_address, node_id)?)?,
        Methods::FindTxOut {
            txid,
            vout,
            script,
            height_hint,
        } => serde_json::to_string_pretty(&client.find_tx_out(
            txid,
            vout,
            script,
            height_hint.unwrap_or(0),
        )?)?,
        Methods::GetMemoryInfo { mode } => {
            let mode = mode.unwrap_or("stats".to_string());
            serde_json::to_string_pretty(&client.get_memory_info(mode)?)?
        }
        Methods::GetRpcInfo => serde_json::to_string_pretty(&client.get_rpc_info()?)?,
        Methods::Uptime => serde_json::to_string_pretty(&client.uptime()?)?,
        Methods::ListDescriptors => serde_json::to_string_pretty(&client.list_descriptors()?)?,
        Methods::Ping => serde_json::to_string_pretty(&client.ping()?)?,
    })
}

#[derive(Debug, Parser)]
#[command(author = "Davidson Souza", version = "0.1.0", about = r#"
    A simple command line interface to the Floresta JSON RPC interface.
"#, long_about = None)]
pub struct Cli {
    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    pub config_file: Option<String>,
    /// Which network should we use
    #[arg(short, long, default_value_t=Network::Bitcoin)]
    pub network: Network,
    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub debug: u8,
    /// The RPC host to connect to
    #[arg(short = 'H', long, value_name = "URL")]
    pub rpc_host: Option<String>,
    /// The RPC username to use
    #[arg(short = 'u', long, value_name = "USERNAME")]
    pub rpc_user: Option<String>,
    /// The RPC password to use
    #[arg(short = 'P', long, value_name = "PASSWORD")]
    pub rpc_password: Option<String>,
    /// An actual RPC command to run
    #[command(subcommand)]
    pub methods: Methods,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Methods {
    #[doc = include_str!("../../../doc/rpc/getblockchaininfo.md")]
    #[command(
        name = "getblockchaininfo",
        about = "Returns an object containing various state info regarding blockchain processing.",
        long_about = Some(include_str!("../../../doc/rpc/getblockchaininfo.md")),
        disable_help_subcommand = true
    )]
    GetBlockchainInfo,

    #[doc = include_str!("../../../doc/rpc/getblockhash.md")]
    #[command(
        name = "getblockhash",
        about = "Returns the hash of the block at the given height.",
        long_about = Some(include_str!("../../../doc/rpc/getblockhash.md")),
        disable_help_subcommand = true
    )]
    GetBlockHash { height: u32 },

    #[doc = include_str!("../../../doc/rpc/getbestblockhash.md")]
    #[command(
        name = "getbestblockhash",
        about = "Returns the hash of the best (tip) block in the most-work chain.",
        long_about = Some(include_str!("../../../doc/rpc/getbestblockhash.md")),
        disable_help_subcommand = true
    )]
    GetBestBlockHash,

    #[doc = include_str!("../../../doc/rpc/getblockcount.md")]
    #[command(
        name = "getblockcount",
        about = "Returns the height of the most-work chain.",
        long_about = Some(include_str!("../../../doc/rpc/getblockcount.md")),
        disable_help_subcommand = true
    )]
    GetBlockCount,

    /// Returns the proof that one or more transactions were included in a block
    #[command(name = "gettxoutproof")]
    GetTxOutProof {
        /// The transaction IDs to prove
        #[arg(required = true, value_parser = crate::parsers::parse_json_array::<Txid>)]
        txids: std::vec::Vec<Txid>, // you need to specify the path of Vec https://github.com/clap-rs/clap/discussions/4695

        /// The block in which to look for the transactions
        #[arg(required = false)]
        blockhash: Option<BlockHash>,
    },

    /// Returns the transaction, assuming it is cached by our watch only wallet
    #[doc = include_str!("../../../doc/rpc/gettransaction.md")]
    #[command(name = "gettransaction")]
    GetTransaction { txid: Txid, verbose: Option<bool> },

    #[doc = include_str!("../../../doc/rpc/rescanblockchain.md")]
    #[command(
        name = "rescanblockchain",
        about = "Sends a request to the node to rescan the blockchain searching for transactions related to the wallet's cached addresses.",
        long_about = Some(include_str!("../../../doc/rpc/rescanblockchain.md")),
        disable_help_subcommand = true
    )]
    RescanBlockchain {
        /// The starting point for the rescan. (optional)
        #[arg(required = false, default_value_t = 0)]
        start_block: u32,

        /// The stopping height for the rescan. (optional)
        #[arg(required = false, default_value_t = 0)]
        stop_block: u32,

        /// Treat the start parameter as a UNIX timestamp instead of block height.
        #[arg(
            short = 't',
            long = "timestamp",
            required = false,
            default_value_t = false
        )]
        use_timestamp: bool,

        #[arg(
            short = 'c',
            long = "confidence",
            required = false,
            default_value_t = RescanConfidence::Medium,
            value_enum
        )]
        confidence: RescanConfidence,
    },

    /// Submits a raw transaction to the network
    #[doc = include_str!("../../../doc/rpc/sendrawtransaction.md")]
    #[command(
        name = "sendrawtransaction",
        about = "Broadcast a serialized transaction to the P2P network",
        long_about = Some(include_str!("../../../doc/rpc/sendrawtransaction.md")),
        disable_help_subcommand = true
    )]
    SendRawTransaction { tx: String },

    /// Returns the block header for the given block hash
    #[command(name = "getblockheader")]
    GetBlockHeader { hash: BlockHash },

    /// Loads a new descriptor to the watch only wallet
    #[doc = include_str!("../../../doc/rpc/loaddescriptor.md")]
    #[command(
        name = "loaddescriptor",
        about = "Loads a descriptor into the watch-only wallet, extracting the address(es) for being cached",
        long_about = Some(include_str!("../../../doc/rpc/loaddescriptor.md")),
        disable_help_subcommand = true
    )]
    LoadDescriptor { desc: String },

    /// Returns the roots of the current utreexo forest
    #[command(name = "getroots")]
    GetRoots,

    /// Returns a block
    #[doc = include_str!("../../../doc/rpc/getblock.md")]
    #[command(
        name = "getblock",
        about = "Returns information about a block",
        long_about = Some(include_str!("../../../doc/rpc/getblock.md")),
        disable_help_subcommand = true
    )]
    GetBlock {
        hash: BlockHash,
        verbosity: Option<u32>,
    },

    /// Returns information about the peers we are connected to
    #[command(name = "getpeerinfo")]
    GetPeerInfo,

    /// Returns the value associated with a UTXO, if it's still not spent.
    /// This function only works properly if we have the compact block filters
    /// feature enabled
    #[command(name = "gettxout")]
    GetTxOut { txid: Txid, vout: u32 },

    /// Request a graceful shutdown of Floresta.
    ///
    /// Result:
    /// "str"    (string) A string with the content 'Floresta stopping'
    #[command(name = "stop")]
    Stop,

    #[doc = include_str!("../../../doc/rpc/addnode.md")]
    #[command(
        name = "addnode",
        about = "Attempts to add or remove a node from the list of addnodes",
        long_about = Some(include_str!("../../../doc/rpc/addnode.md")),
        disable_help_subcommand = true
    )]
    AddNode {
        node: String,
        command: AddNodeCommand,
        v2transport: Option<bool>,
    },

    #[doc = include_str!("../../../doc/rpc/disconnectnode.md")]
    #[command(
        name = "disconnectnode",
        about = "Immediately disconnect from a peer",
        long_about = Some(include_str!("../../../doc/rpc/disconnectnode.md")),
        disable_help_subcommand = true
    )]
    DisconnectNode {
        node_address: String,
        node_id: Option<usize>,
    },

    #[command(name = "findtxout")]
    FindTxOut {
        txid: Txid,
        vout: u32,
        script: String,
        height_hint: Option<u32>,
    },

    /// Returns statistics about Floresta's memory usage.
    ///
    /// Returns zeroed values for all runtimes that are not *-gnu or MacOS.
    #[command(name = "getmemoryinfo")]
    GetMemoryInfo { mode: Option<String> },

    /// Returns information about the RPC server
    ///
    /// Result: {                  (json object)
    ///   "active_commands" : [    (json array) All active commands
    ///     {                      (json object) Information about an active command
    ///       "method" : "str",    (string) The name of the RPC command
    ///       "duration" : n       (numeric) The running time in microseconds
    ///     },
    ///     ...
    ///   ],
    ///   "logpath" : "str"        (string) The complete file path to the debug log
    /// }
    #[command(name = "getrpcinfo")]
    GetRpcInfo,

    /// Returns for how long the node has been running, in seconds
    #[command(name = "uptime")]
    Uptime,

    /// Returns a list of all descriptors currently loaded in the wallet
    #[command(name = "listdescriptors")]
    ListDescriptors,

    /// Sends a ping to all peers, checking if they are still alive
    ///
    /// Result: json null
    #[command(name = "ping")]
    Ping,
}
