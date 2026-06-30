// SPDX-License-Identifier: MIT OR Apache-2.0

use bitcoin::Block;
use bitcoin::OutPoint;
use bitcoin::ScriptBuf;
use bitcoin::Transaction;
use bitcoin::TxOut;
use bitcoin::hash_types::Txid;
use bitcoin::hashes::sha256;

use super::error::WatchOnlyError;
use super::model::CachedTransaction;
use super::model::MerkleProof;
use super::model::Stats;

/// Public trait defining a common interface for address cache implementations.
///
/// This trait abstracts the wallet's address caching functionality, allowing different
/// database backends to be used while maintaining a consistent interface.
pub trait WalletBase: Sync + Send + 'static {
    /// Returns the UTXO at the given outpoint, if it exists and is unspent
    fn get_utxo(&self, outpoint: &OutPoint) -> Result<Option<TxOut>, WatchOnlyError>;

    /// Returns the number of cached addresses
    fn n_cached_addresses(&self) -> Result<usize, WatchOnlyError>;

    /// Returns the balance of the address with the given script hash
    fn get_address_balance(
        &self,
        script_hash: &sha256::Hash,
    ) -> Result<Option<u64>, WatchOnlyError>;

    /// Returns all cached addresses' scripts
    fn get_cached_addresses(&self) -> Result<Vec<ScriptBuf>, WatchOnlyError>;

    /// Sets the cache height to the given value
    fn bump_height(&self, height: u32) -> Result<(), WatchOnlyError>;

    /// Returns the current cache height
    fn get_cache_height(&self) -> Result<u32, WatchOnlyError>;

    /// Checks if a descriptor is already cached
    fn is_cached(&self, desc: &str) -> Result<bool, WatchOnlyError>;

    /// Checks if an address is already cached
    fn is_address_cached(&self, script_hash: &sha256::Hash) -> Result<bool, WatchOnlyError>;

    /// Pushes a descriptor to the wallet, returning an error if it's already cached
    fn push_descriptor(&self, descriptor: &str) -> Result<Vec<ScriptBuf>, WatchOnlyError>;

    /// Adds an XPUB to the wallet, derives descriptors and caches addresses
    fn push_xpub(&self, xpub: &str, network: bitcoin::Network) -> Result<(), WatchOnlyError>;

    /// Returns the position of a transaction in the block
    fn get_position(&self, txid: &Txid) -> Result<Option<u32>, WatchOnlyError>;

    /// Returns the height of a transaction
    fn get_height(&self, txid: &Txid) -> Result<Option<u32>, WatchOnlyError>;

    /// Returns a cached transaction as a hex string
    fn get_cached_transaction(&self, txid: &Txid) -> Result<Option<String>, WatchOnlyError>;

    /// Initializes the wallet setup
    fn setup(&self) -> Result<(), WatchOnlyError>;

    /// Processes a block, looking for transactions related to our addresses
    fn block_process(
        &self,
        block: &Block,
        height: u32,
    ) -> Result<Vec<(Transaction, TxOut)>, WatchOnlyError>;

    /// Returns UTXOs for the given script hash
    fn get_address_utxos(
        &self,
        script_hash: &sha256::Hash,
    ) -> Result<Option<Vec<(TxOut, OutPoint)>>, WatchOnlyError>;

    /// Returns a cached transaction by its txid
    fn get_transaction(&self, txid: &Txid) -> Result<Option<CachedTransaction>, WatchOnlyError>;

    /// Returns the transaction history for an address
    fn get_address_history(
        &self,
        script_hash: &sha256::Hash,
    ) -> Result<Option<Vec<CachedTransaction>>, WatchOnlyError>;

    /// Returns the Merkle proof for a transaction
    fn get_merkle_proof(&self, txid: &Txid) -> Result<Option<MerkleProof>, WatchOnlyError>;

    /// Derives new addresses from cached descriptors
    fn derive_addresses(&self) -> Result<(), WatchOnlyError>;

    /// Returns wallet statistics
    fn get_stats(&self) -> Result<Stats, WatchOnlyError>;

    /// Derives addresses if needed based on transaction count
    fn maybe_derive_addresses(&self) -> Result<(), WatchOnlyError>;

    /// Finds all unconfirmed transactions
    fn find_unconfirmed(&self) -> Result<Vec<Transaction>, WatchOnlyError>;

    /// Caches a new address
    fn cache_address(&self, script_pk: ScriptBuf) -> Result<(), WatchOnlyError>;

    /// Caches a mempool transaction
    fn cache_mempool_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<Vec<TxOut>, WatchOnlyError>;

    /// Saves a mempool transaction
    fn save_mempool_tx(
        &self,
        hash: sha256::Hash,
        transaction_to_cache: CachedTransaction,
    ) -> Result<(), WatchOnlyError>;

    /// Saves a non-mempool transaction
    fn save_non_mempool_tx(
        &self,
        transaction: &Transaction,
        is_spend: bool,
        value: u64,
        index: usize,
        hash: sha256::Hash,
        transaction_to_cache: CachedTransaction,
    ) -> Result<(), WatchOnlyError>;

    /// Returns all cached descriptors
    fn get_descriptors(&self) -> Result<Vec<String>, WatchOnlyError>;

    #[allow(clippy::too_many_arguments)]
    /// Caches a transaction
    fn cache_transaction(
        &self,
        transaction: &Transaction,
        height: u32,
        value: u64,
        merkle_block: MerkleProof,
        position: u32,
        index: usize,
        is_spend: bool,
        hash: sha256::Hash,
    ) -> Result<(), WatchOnlyError>;
}
