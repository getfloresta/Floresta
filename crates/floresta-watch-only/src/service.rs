// SPDX-License-Identifier: MIT OR Apache-2.0

#![deny(clippy::unwrap_used)]

use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;
use std::collections::HashMap;
use std::sync::RwLock;

use bitcoin::consensus::encode::serialize_hex;
use bitcoin::hashes::sha256::Hash;
use bitcoin::hashes::Hash as HashTrait;
use bitcoin::Address;
use bitcoin::Amount;
use bitcoin::Block;
#[cfg(all(feature = "bdk-provider", feature = "sqlite"))]
use bitcoin::Network;
use bitcoin::OutPoint;
use bitcoin::ScriptBuf;
use bitcoin::Transaction;
use bitcoin::TxOut;
use bitcoin::Txid;
use floresta_chain::BlockConsumer;
use floresta_chain::UtxoData;
use floresta_common::get_spk_hash;
use floresta_common::impl_error_from;
use tracing::error;

use crate::merkle::MerkleProof;
use crate::metadata::DescriptorInfoMetadata;
use crate::metadata::WalletMetadata;
use crate::metadata::WalletMetadataError;
use crate::models::Balance;
use crate::models::GetBalanceParams;
use crate::models::ImportDescriptor;
#[cfg(all(feature = "bdk-provider", feature = "sqlite"))]
use crate::provider::new_provider;
use crate::provider::WalletProvider;
use crate::provider::WalletProviderError;
use crate::provider::WalletProviderEvent;
#[cfg(all(feature = "bdk-provider", feature = "sqlite"))]
use crate::repository::new_repository;
use crate::repository::DbDescriptor;
use crate::repository::DbScriptBuffer;
use crate::repository::DbTransaction;
use crate::repository::WalletRepository;
use crate::repository::WalletRepositoryError;

#[cfg(all(feature = "bdk-provider", feature = "sqlite"))]
pub fn new_wallet(datadir: &str, network: Network) -> Result<Box<dyn Wallet>, WalletServiceError> {
    let service = WalletService::new_default(datadir, network)?;

    Ok(Box::new(service))
}

#[cfg(all(feature = "bdk-provider", feature = "sqlite"))]
pub fn new_block_consumer(
    datadir: &str,
    network: Network,
) -> Result<Box<dyn BlockConsumer>, WalletServiceError> {
    let service = WalletService::new_default(datadir, network)?;

    Ok(Box::new(service))
}

pub struct WalletService {
    provider: RwLock<Box<dyn WalletProvider>>,
    persister: Box<dyn WalletRepository>,
    metadata: RwLock<WalletMetadata>,
}

impl WalletService {
    pub fn new(provider: Box<dyn WalletProvider>, persister: Box<dyn WalletRepository>) -> Self {
        let metadata = WalletMetadata::default();

        Self {
            provider: RwLock::new(provider),
            persister,
            metadata: RwLock::new(metadata),
        }
    }

    #[cfg(all(feature = "bdk-provider", feature = "sqlite"))]
    pub fn new_default(datadir: &str, network: Network) -> Result<Self, WalletServiceError> {
        let persister_datadir = format!("{datadir}/repository.db3");
        let persister = new_repository(&persister_datadir)?;

        let is_wallet_initialized = persister.exists_descriptor(None, None).unwrap_or(false);

        let provider_datadir = format!("{datadir}/provider.db3");
        let provider = new_provider(&provider_datadir, network, is_wallet_initialized)?;

        Ok(Self::new(provider, persister))
    }

    fn get_provider(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, Box<dyn WalletProvider>>, WalletServiceError> {
        self.provider
            .read()
            .map_err(|e| WalletServiceError::LockPoisoned(e.to_string()))
    }

    fn get_provider_mut(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, Box<dyn WalletProvider>>, WalletServiceError> {
        self.provider
            .write()
            .map_err(|e| WalletServiceError::LockPoisoned(e.to_string()))
    }

    fn get_metadata(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, WalletMetadata>, WalletServiceError> {
        let metadata = self
            .metadata
            .read()
            .map_err(|e| WalletServiceError::LockPoisoned(e.to_string()))?;

        if metadata.name.is_empty() {
            return Err(WalletServiceError::WalletNotLoaded);
        }

        Ok(metadata)
    }

    fn get_metadata_mut(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, WalletMetadata>, WalletServiceError> {
        let metadata = self
            .metadata
            .write()
            .map_err(|e| WalletServiceError::LockPoisoned(e.to_string()))?;

        if metadata.name.is_empty() {
            return Err(WalletServiceError::WalletNotLoaded);
        }

        Ok(metadata)
    }

    fn get_metadata_mut_not_validated(
        &self,
    ) -> Result<std::sync::RwLockWriteGuard<'_, WalletMetadata>, WalletServiceError> {
        self.metadata
            .write()
            .map_err(|e| WalletServiceError::LockPoisoned(e.to_string()))
    }

    fn process_block_inner(
        &self,
        block: &Block,
        height: u32,
    ) -> Result<Vec<(Transaction, TxOut)>, WalletServiceError> {
        let provider = self.get_provider()?;
        let events = provider
            .block_process(block, height)
            .map_err(WalletServiceError::ProviderError)?;

        self.process_event(events, Some(block), Some(height as u64))
    }

    fn process_event(
        &self,
        event: Vec<WalletProviderEvent>,
        block: Option<&Block>,
        height: Option<u64>,
    ) -> Result<Vec<(Transaction, TxOut)>, WalletServiceError> {
        let mut transaction_update = Vec::new();
        for e in event {
            match e {
                WalletProviderEvent::UpdateTransaction { tx, output } => {
                    // Persist the script buffer of this transaction output that we know about
                    let hash = get_spk_hash(&output.script_pubkey);
                    let script_info = DbScriptBuffer {
                        script: output.script_pubkey.clone(),
                        hash,
                    };
                    self.persister
                        .insert_or_update_script_buffer(&script_info)?;

                    // Add the transaction to the list of transactions to update in the wallet state
                    transaction_update.push((tx, output));
                }
                WalletProviderEvent::ConfirmedTransaction { tx } => {
                    let block = block.ok_or_else(|| {
                        WalletServiceError::BlockProcessingError(
                            "Block must be provided for TxConfirmed event".to_string(),
                        )
                    })?;
                    if height.is_none() {
                        return Err(WalletServiceError::BlockProcessingError(
                            "Height must be provided for TxConfirmed event".to_string(),
                        ));
                    }

                    let position = self.get_transaction_position(&tx.compute_txid(), block)?;

                    let proof = MerkleProof::from_block(block, position);

                    let tx_persist = DbTransaction {
                        hash: tx.compute_txid(),
                        tx,
                        merkle_block: Some(proof),
                        height,
                        position: Some(position),
                    };

                    self.persister.insert_or_update_transaction(&tx_persist)?;
                }
                WalletProviderEvent::UnconfirmedTransactionInBlock { tx } => {
                    let tx_persist = DbTransaction {
                        hash: tx.compute_txid(),
                        tx,
                        merkle_block: None,
                        height: None,
                        position: None,
                    };

                    self.persister.insert_or_update_transaction(&tx_persist)?;
                }
            }
        }

        Ok(transaction_update)
    }

    fn get_transaction_position(
        &self,
        txid: &Txid,
        block: &Block,
    ) -> Result<u64, WalletProviderError> {
        block
            .txdata
            .iter()
            .position(|tx| &tx.compute_txid() == txid)
            .map(|pos| pos as u64)
            .ok_or(WalletProviderError::TransactionNotFoundInBlock(*txid))
    }
}

impl BlockConsumer for WalletService {
    fn wants_spent_utxos(&self) -> bool {
        false
    }

    fn on_block(
        &self,
        block: &Block,
        height: u32,
        _spent_utxos: Option<&HashMap<OutPoint, UtxoData>>,
    ) {
        // We only process block if the wallet is initialized,
        if self
            .persister
            .exists_descriptor(None, None)
            .unwrap_or(false)
        {
            return;
        }

        self.process_block_inner(block, height).unwrap_or_else(|e| {
            error!("Error processing block({height}): {e:?}");
            Vec::new()
        });
    }
}

#[derive(Debug)]
pub enum WalletServiceError {
    ProviderError(WalletProviderError),

    PersistError(WalletRepositoryError),

    MetadataError(WalletMetadataError),

    LockPoisoned(String),

    BlockProcessingError(String),

    NotFound(String),

    WalletNotLoaded,
}

//impl display error
impl Display for WalletServiceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            WalletServiceError::ProviderError(e) => write!(f, "Provider error: {e}"),
            WalletServiceError::PersistError(e) => write!(f, "Persistence error: {e}"),
            WalletServiceError::MetadataError(e) => write!(f, "Metadata error: {e}"),
            WalletServiceError::LockPoisoned(e) => write!(f, "Lock poisoned: {e}"),
            WalletServiceError::BlockProcessingError(e) => {
                write!(f, "Block processing error: {e}")
            }
            WalletServiceError::NotFound(e) => write!(f, "Not found: {e}"),
            WalletServiceError::WalletNotLoaded => write!(f, "Wallet not loaded"),
        }
    }
}

impl_error_from!(WalletServiceError, WalletProviderError, ProviderError);
impl_error_from!(WalletServiceError, WalletRepositoryError, PersistError);
impl_error_from!(WalletServiceError, WalletMetadataError, MetadataError);

pub trait Wallet {
    // Event processing functions

    // Process transactions in a block and update the wallet state accordingly.
    fn process_block(
        &self,
        block: &Block,
        height: u32,
    ) -> Result<Vec<(Transaction, TxOut)>, WalletServiceError>;

    // Process mempool transactions and update the wallet state accordingly.
    fn process_mempool_transactions(
        &self,
        transactions: Vec<&Transaction>,
    ) -> Result<Vec<TxOut>, WalletServiceError>;

    // Get the UTXO for a given outpoint, if it belongs to the wallet and is unspent.
    fn get_utxo(&self, outpoint: &OutPoint) -> Result<Option<TxOut>, WalletServiceError>;

    // Get the transaction details for a given transaction ID
    fn get_transaction(&self, txid: &Txid) -> Result<Option<DbTransaction>, WalletServiceError>;

    // Get the transaction history for a given address/script hash
    fn get_address_history(
        &self,
        script_hash: &Hash,
    ) -> Result<Option<Vec<DbTransaction>>, WalletServiceError>;

    // Get the balance for a given address/script hash
    fn get_address_balance(&self, script_hash: &Hash) -> Result<u64, WalletServiceError>;

    // Get a list of all addresses currently in the wallet
    fn get_cached_addresses(&self) -> Result<Vec<ScriptBuf>, WalletServiceError>;

    // Get a list of all UTXOs currently in script hash, along with their outpoints
    fn get_address_utxos(
        &self,
        script_hash: &Hash,
    ) -> Result<Option<Vec<(TxOut, OutPoint)>>, WalletServiceError>;

    // Get the Merkle proof for a given transaction ID, if it is confirmed in a block.
    fn get_merkle_proof(&self, txid: &Txid) -> Result<Option<MerkleProof>, WalletServiceError>;

    // Get the position of a transaction within its block, if it is confirmed.
    fn get_position(&self, txid: &Txid) -> Result<Option<u64>, WalletServiceError>;

    // Get the block height at which a transaction was confirmed, if it is confirmed.
    fn get_height(&self, txid: &Txid) -> Result<Option<u64>, WalletServiceError>;

    // Get the raw transaction hex for a given transaction ID, if it is known to the wallet.
    fn get_cached_transaction(&self, txid: &Txid) -> Result<Option<String>, WalletServiceError>;

    // Create a new wallet with the given name. This will persist in repository and loaded in memory.
    fn create_wallet(&self, wallet: &str) -> Result<(), WalletServiceError>;

    // Load an existing wallet by name. This will persist in memory and be used for all subsequent operations.
    // If the wallet does not exist, an error is returned.
    fn load_wallet(&self, wallet: &str) -> Result<(), WalletServiceError>;

    // Push a new descriptor to the wallet. Needed to load a wallet or create a new wallet.
    fn push_descriptor(&self, descriptor: &ImportDescriptor) -> Result<(), WalletServiceError>;

    // Get a list of all descriptors currently in the wallet, along with their metadata.
    fn get_descriptors(&self) -> Result<Vec<String>, WalletServiceError>;

    // Generate a new address from the wallet. If is_change is true, generates a change address.
    fn new_address(&self, is_change: bool) -> Result<Address, WalletServiceError>;

    // Find all unconfirmed transactions currently in the wallet. This is used to populate the mempool state on startup.
    fn find_unconfirmed(&self) -> Result<Vec<Transaction>, WalletServiceError>;

    // Get the total balance of the wallet, with options to filter by minimum confirmations and avoid_reuse.
    fn get_balance(&self, params: GetBalanceParams) -> Result<Amount, WalletServiceError>;

    // Get the balances of all wallets. This includes the trusted, untrusted pending, immature and
    // used balances, along with the last processed block information.
    fn get_balances(&self) -> Result<Balance, WalletServiceError>;
}

impl Wallet for WalletService {
    // Event processing functions

    fn process_block(
        &self,
        block: &Block,
        height: u32,
    ) -> Result<Vec<(Transaction, TxOut)>, WalletServiceError> {
        self.process_block_inner(block, height)
    }

    fn process_mempool_transactions(
        &self,
        transactions: Vec<&Transaction>,
    ) -> Result<Vec<TxOut>, WalletServiceError> {
        let provider = self.get_provider()?;

        let events = provider.process_mempool_transactions(transactions)?;

        let vec = self.process_event(events, None, None)?;

        Ok(vec.into_iter().map(|(_, output)| output).collect())
    }

    // Data retrieval functions

    fn get_utxo(&self, outpoint: &OutPoint) -> Result<Option<TxOut>, WalletServiceError> {
        let provider = self.get_provider()?;

        let tx_out = provider.get_txo(outpoint, Some(false))?;

        Ok(tx_out)
    }

    fn get_transaction(&self, txid: &Txid) -> Result<Option<DbTransaction>, WalletServiceError> {
        let tx = self.persister.get_transaction(txid);

        match tx {
            Ok(tx) => Ok(Some(tx)),
            Err(WalletRepositoryError::NotFound(_)) => Ok(None),
            Err(e) => Err(WalletServiceError::PersistError(e)),
        }
    }

    fn get_address_history(
        &self,
        script_hash: &Hash,
    ) -> Result<Option<Vec<DbTransaction>>, WalletServiceError> {
        let scriptt_info = self.persister.get_script_buffer(script_hash)?;

        let outpoints = self
            .get_provider()?
            .get_local_output_by_script(scriptt_info.script, Some(true))?;

        let mut transactions = Vec::new();
        for outpoint in outpoints {
            let tx = self.persister.get_transaction(&outpoint.outpoint.txid)?;
            transactions.push(tx);
        }

        transactions.sort_by_key(|tx| tx.height.unwrap_or(0));

        Ok(Some(transactions))
    }

    fn get_address_balance(&self, hash: &Hash) -> Result<u64, WalletServiceError> {
        let provider = self.get_provider()?;

        let scriptt_info = self.persister.get_script_buffer(hash)?;

        let outpoints = provider.get_local_output_by_script(scriptt_info.script, Some(false))?;

        let balance = outpoints.iter().map(|o| o.txout.value.to_sat()).sum();

        Ok(balance)
    }

    fn get_cached_addresses(&self) -> Result<Vec<ScriptBuf>, WalletServiceError> {
        let provider = self.get_provider()?;

        let spk = provider.list_script_buff(None)?;

        Ok(spk)
    }

    fn get_address_utxos(
        &self,
        script_hash: &Hash,
    ) -> Result<Option<Vec<(TxOut, OutPoint)>>, WalletServiceError> {
        let scriptt_info = self.persister.get_script_buffer(script_hash)?;

        let outpoints = self
            .get_provider()?
            .get_local_output_by_script(scriptt_info.script, Some(false))?;

        let utxos = outpoints
            .into_iter()
            .map(|o| (o.txout, o.outpoint))
            .collect();

        Ok(Some(utxos))
    }

    fn get_merkle_proof(&self, txid: &Txid) -> Result<Option<MerkleProof>, WalletServiceError> {
        let tx = self.persister.get_transaction(txid)?;

        Ok(tx.merkle_block)
    }

    fn get_position(&self, txid: &Txid) -> Result<Option<u64>, WalletServiceError> {
        let tx = self.persister.get_transaction(txid)?;

        Ok(tx.position)
    }

    fn get_height(&self, txid: &Txid) -> Result<Option<u64>, WalletServiceError> {
        let tx = self.persister.get_transaction(txid)?;

        Ok(tx.height)
    }

    fn get_cached_transaction(&self, txid: &Txid) -> Result<Option<String>, WalletServiceError> {
        let tx = self.get_transaction(txid)?;

        Ok(tx.map(|tx| serialize_hex(&tx.tx)))
    }

    // Wallet management functions

    fn create_wallet(&self, wallet: &str) -> Result<(), WalletServiceError> {
        self.persister.create_wallet(wallet)?;

        self.load_wallet(wallet)
    }

    fn load_wallet(&self, wallet: &str) -> Result<(), WalletServiceError> {
        let descriptor = self.persister.load_wallet(wallet)?;

        let mut active_external = None;
        let mut active_internal = None;
        let mut descriptos_metadata = Vec::new();
        for desc in descriptor {
            let metadata = db_descriptor_to_metadata(&desc);

            if desc.is_active {
                if desc.is_change {
                    active_internal = Some(metadata);
                } else {
                    active_external = Some(metadata);
                }
            } else {
                descriptos_metadata.push(metadata);
            }
        }

        let wallet_metadata = WalletMetadata::new(
            wallet,
            active_external,
            active_internal,
            descriptos_metadata,
        );

        let mut metadata = self.get_metadata_mut_not_validated()?;
        *metadata = wallet_metadata;

        Ok(())
    }

    fn push_descriptor(
        &self,
        import_descriptor: &ImportDescriptor,
    ) -> Result<(), WalletServiceError> {
        let wallet_name;
        {
            let mut metadata = self.get_metadata_mut()?;
            wallet_name = metadata.name.clone();

            let descriptor = DbDescriptor {
                wallet: metadata.name.clone(),
                id: generate_id_for_descriptor(&import_descriptor.descriptor),
                descriptor: import_descriptor.descriptor.clone(),
                label: import_descriptor.label.clone(),
                is_active: import_descriptor.is_active,
                is_change: import_descriptor.is_change,
            };

            let existing_descriptor = self
                .persister
                .exists_descriptor(Some(&descriptor.id), None)?;

            if !existing_descriptor {
                self.get_provider_mut()?
                    .persist_descriptor(&descriptor.id, &descriptor.descriptor)?;
            }
            self.persister.insert_or_update_descriptor(&descriptor)?;

            let desc_metadata = db_descriptor_to_metadata(&descriptor);
            let replace_desc = metadata.add_descriptor(
                desc_metadata,
                descriptor.is_change,
                descriptor.is_active,
            )?;

            if let Some(replace_desc) = replace_desc {
                self.persister.insert_or_update_descriptor(&DbDescriptor {
                    descriptor: replace_desc.descriptor,
                    id: replace_desc.id,
                    label: replace_desc.label,
                    wallet: metadata.name.clone(),
                    is_change: descriptor.is_change,
                    is_active: false,
                })?;
            }
        }

        self.load_wallet(&wallet_name)
    }

    fn get_descriptors(&self) -> Result<Vec<String>, WalletServiceError> {
        let descriptors = self
            .get_metadata()?
            .get_descriptors()
            .iter()
            .map(|desc| desc.descriptor.clone())
            .collect();

        Ok(descriptors)
    }

    fn new_address(&self, is_change: bool) -> Result<Address, WalletServiceError> {
        let metadata = self.get_metadata()?;
        let descriptor = metadata.get_active_descriptor(is_change)?;

        let provider = self.get_provider()?;
        let address = provider.new_address(&descriptor.id)?;

        Ok(address)
    }

    fn find_unconfirmed(&self) -> Result<Vec<Transaction>, WalletServiceError> {
        let txs = self.persister.list_transactions()?;

        Ok(txs
            .iter()
            .filter(|tx| tx.height.is_none())
            .map(|tx| tx.tx.clone())
            .collect())
    }

    fn get_balance(&self, params: GetBalanceParams) -> Result<Amount, WalletServiceError> {
        let provider = self.get_provider()?;

        let metadata = self.get_metadata()?;

        let balance = provider.get_balance(metadata.get_ids(), params)?;

        Ok(balance)
    }

    fn get_balances(&self) -> Result<Balance, WalletServiceError> {
        let provider = self.get_provider()?;

        let metadata = self.get_metadata()?;

        let balances = provider.get_balances(metadata.get_ids())?;

        Ok(balances)
    }
}

fn db_descriptor_to_metadata(desc: &DbDescriptor) -> DescriptorInfoMetadata {
    DescriptorInfoMetadata {
        descriptor: desc.descriptor.clone(),
        id: desc.id.clone(),
        label: desc.label.clone(),
    }
}

fn generate_id_for_descriptor(desc: &str) -> String {
    let hash = Hash::hash(desc.as_bytes());

    hash.to_string()
}
