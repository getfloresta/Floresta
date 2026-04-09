// SPDX-License-Identifier: MIT OR Apache-2.0

#![deny(clippy::unwrap_used)]

use core::fmt;
use core::fmt::Debug;
use core::fmt::Display;
use core::fmt::Formatter;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::result;
use std::sync::Mutex;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;
use std::sync::RwLockWriteGuard;

use bdk_wallet::chain::local_chain::CannotConnectError;
use bdk_wallet::keyring::KeyRing;
use bdk_wallet::keyring::KeyRingError;
use bdk_wallet::rusqlite::types::FromSql;
use bdk_wallet::rusqlite::types::FromSqlError;
use bdk_wallet::rusqlite::types::ToSql;
use bdk_wallet::rusqlite::types::ToSqlOutput;
use bdk_wallet::rusqlite::types::ValueRef;
use bdk_wallet::rusqlite::Connection;
use bdk_wallet::rusqlite::Error as RusqliteError;
use bdk_wallet::CreateWithPersistError;
use bdk_wallet::LoadWithPersistError;
use bdk_wallet::PersistedWallet;
use bdk_wallet::Wallet;
use bdk_wallet::WalletEvent;
use bdk_wallet::WalletPersister;
use bitcoin::Address;
use bitcoin::Amount;
use bitcoin::Block;
use bitcoin::Network;
use bitcoin::OutPoint;
use bitcoin::ScriptBuf;
use bitcoin::Transaction;
use bitcoin::TxOut;
use bitcoin::Txid;
use floresta_common::prelude::sync::Arc;

use super::Balance;
use super::LastProcessedBlock;
use super::LocalOutput;
use super::WalletProviderError;
use super::WalletProviderEvent;

#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Debug)]
pub struct KeyId(String);

impl From<String> for KeyId {
    fn from(s: String) -> Self {
        KeyId(s)
    }
}

impl Display for KeyId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl ToSql for KeyId {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, RusqliteError> {
        Ok(ToSqlOutput::from(self.0.clone()))
    }
}

impl FromSql for KeyId {
    fn column_result(value: ValueRef) -> Result<KeyId, FromSqlError> {
        String::column_result(value).map(KeyId)
    }
}

pub struct BdkWalletProvider<P, K>
where
    K: Ord + Clone + Debug + From<String> + Send,
    P: WalletPersister<K> + Send,
{
    wallet: Option<RwLock<PersistedWallet<P, K>>>,
    persister: Arc<Mutex<P>>,
    network: Network,
}

impl<K> BdkWalletProvider<Connection, K>
where
    K: Ord + Clone + Debug + ToSql + FromSql + From<String> + Display + 'static + Send + Sync,
{
    pub(crate) fn new(
        db_path: &str,
        network: Network,
        is_initialized: bool,
    ) -> Result<Self, WalletProviderError> {
        let persister = Connection::open(db_path).map_err(|e| {
            WalletProviderError::PersistenceError(format!("Failed to open db: {}", e))
        })?;

        Self::setup(persister, network, is_initialized)
    }

    #[cfg(test)]
    pub(crate) fn new_in_memory(network: Network) -> Result<Self, WalletProviderError> {
        let persister = Connection::open_in_memory().map_err(|e| {
            WalletProviderError::PersistenceError(format!("Failed to create in-memory db: {}", e))
        })?;

        Self::setup(persister, network, false)
    }

    fn setup(
        persister: Connection,
        network: Network,
        is_initialized: bool,
    ) -> Result<Self, WalletProviderError> {
        if is_initialized {
            return Self::load_wallet_from_sqlite(persister);
        }

        Ok(Self {
            wallet: None,
            persister: Arc::new(Mutex::new(persister)),
            network,
        })
    }

    fn load_wallet_from_sqlite(mut persister: Connection) -> Result<Self, WalletProviderError> {
        let wallet = Wallet::load().load_wallet(&mut persister)?.ok_or_else(|| {
            WalletProviderError::WalletLoadError("Option wallet is None".to_string())
        })?;

        Ok(Self {
            wallet: Some(RwLock::new(wallet)),
            persister: Arc::new(Mutex::new(persister)),
            network: Network::Bitcoin,
        })
    }
}

impl<P, K> BdkWalletProvider<P, K>
where
    K: Ord + Clone + Debug + From<String> + Send + Sync,
    P: WalletPersister<K> + Send,
{
    fn initialize_wallet(&mut self, id: &str, descriptor: &str) -> Result<(), WalletProviderError> {
        let wallet = {
            let mut persister = self.get_persister()?;

            let keyring = KeyRing::new_with_descriptors(
                self.network,
                BTreeMap::from([(K::from(id.to_string()), descriptor.to_string())]),
            )?;

            Wallet::create(keyring).create_wallet(&mut *persister)?
        };

        self.wallet = Some(RwLock::new(wallet));
        Ok(())
    }

    fn get_wallet(
        &self,
    ) -> Result<RwLockReadGuard<'_, PersistedWallet<P, K>>, WalletProviderError> {
        if let Some(wallet) = &self.wallet {
            wallet
                .read()
                .map_err(|e| WalletProviderError::LockPoisoned(e.to_string()))
        } else {
            Err(WalletProviderError::WalletNotInitialized)
        }
    }

    fn get_wallet_mut(
        &self,
    ) -> Result<RwLockWriteGuard<'_, PersistedWallet<P, K>>, WalletProviderError> {
        if let Some(wallet) = &self.wallet {
            wallet
                .write()
                .map_err(|e| WalletProviderError::LockPoisoned(e.to_string()))
        } else {
            Err(WalletProviderError::WalletNotInitialized)
        }
    }

    fn get_persister(&self) -> result::Result<std::sync::MutexGuard<'_, P>, WalletProviderError> {
        self.persister
            .lock()
            .map_err(|e| WalletProviderError::LockPoisoned(e.to_string()))
    }

    fn event_process(
        &self,
        events: Vec<WalletEvent>,
    ) -> Result<Vec<WalletProviderEvent>, WalletProviderError> {
        let mut result_events = Vec::new();

        for event in events {
            match event {
                WalletEvent::ChainTipChanged {
                    old_tip: _,
                    new_tip: _,
                } => {}
                WalletEvent::TxConfirmed {
                    txid: _,
                    tx,
                    block_time: _,
                    old_block_time: _,
                } => {
                    result_events.extend(self.get_owned_transaction_outputs(&tx)?);

                    result_events
                        .push(WalletProviderEvent::ConfirmedTransaction { tx: (*tx).clone() });
                }
                WalletEvent::TxUnconfirmed {
                    txid: _,
                    tx,
                    old_block_time: _,
                } => {
                    result_events.extend(self.get_owned_transaction_outputs(&tx)?);

                    result_events.push(WalletProviderEvent::UnconfirmedTransactionInBlock {
                        tx: (*tx).clone(),
                    });
                }
                WalletEvent::TxDropped { txid: _, tx } => {
                    result_events.extend(self.get_owned_transaction_outputs(&tx)?);

                    result_events.push(WalletProviderEvent::UnconfirmedTransactionInBlock {
                        tx: (*tx).clone(),
                    });
                }
                WalletEvent::TxReplaced {
                    txid: _,
                    tx,
                    conflicts: _,
                } => {
                    result_events.extend(self.get_owned_transaction_outputs(&tx)?);

                    result_events.push(WalletProviderEvent::UnconfirmedTransactionInBlock {
                        tx: (*tx).clone(),
                    });
                }
                _other => {}
            }
        }

        Ok(result_events)
    }

    fn get_owned_transaction_outputs(
        &self,
        transaction: &Transaction,
    ) -> Result<Vec<WalletProviderEvent>, WalletProviderError> {
        let wallet = self.get_wallet()?;

        let events = transaction
            .output
            .iter()
            .filter(|out| wallet.is_mine(out.script_pubkey.clone()))
            .map(|out| WalletProviderEvent::UpdateTransaction {
                output: out.clone(),
                tx: transaction.clone(),
            })
            .collect();

        Ok(events)
    }
}

impl<K, P> super::WalletProvider for BdkWalletProvider<P, K>
where
    K: Ord + Clone + Debug + From<String> + ToString + Send + Sync,
    P: WalletPersister<K> + Send,
{
    fn block_process(
        &self,
        block: &Block,
        height: u32,
    ) -> Result<Vec<WalletProviderEvent>, WalletProviderError> {
        let mut wallet = self.get_wallet_mut()?;

        let events = wallet.apply_block_events(block, height)?;

        wallet.persist(&mut *self.get_persister()?).map_err(|_| {
            WalletProviderError::PersistenceError(
                "Error persist the wallet after applying block events".to_string(),
            )
        })?;

        drop(wallet);

        self.event_process(events)
    }

    fn persist_descriptor(
        &mut self,
        id: &str,
        descriptor: &str,
    ) -> Result<(), WalletProviderError> {
        // if wallet is not initialized, initialize it with the provided descriptor. Otherwise, add the
        if self.wallet.is_none() {
            self.initialize_wallet(id, descriptor)?;
            return Ok(());
        }

        // Add the descriptor to the keyring and persist it, then reload the wallet to pick up the
        // new descriptor. We have to do this dance because the BDK wallet doesn't support adding
        // descriptors at runtime, so we have to persist the new keyring and then reload the wallet
        // to pick it up.
        {
            let wallet = self.get_wallet()?;
            let mut keyring = wallet.keyring().clone();
            if keyring.list_keychains().keys().any(|k| k.to_string() == id) {
                return Err(WalletProviderError::DescriptorAlreadyExists(format!(
                    "Descriptor with id {id} already exists in provider"
                )));
            }
            let change_keyring =
                keyring.add_descriptor(id.to_string().into(), descriptor.to_string())?;

            let changeset = bdk_wallet::ChangeSet {
                keyring: change_keyring,
                ..Default::default()
            };

            let mut persister = self.get_persister()?;

            P::persist(&mut *persister, &changeset).map_err(|_| {
                WalletProviderError::PersistenceError("Error persisting keyring".to_string())
            })?;
        } // Drop wallet read lock before acquiring write lock in next step

        // Now reload the wallet to pick up the new descriptor
        {
            let mut wallet = self.get_wallet_mut()?;
            let mut persister = self.get_persister()?;

            let new_wallet = Wallet::load()
                .load_wallet(&mut *persister)
                .map_err(|_| {
                    WalletProviderError::WalletLoadError("Error loading wallet".to_string())
                })?
                .ok_or_else(|| {
                    WalletProviderError::WalletLoadError("Option wallet is None".to_string())
                })?;

            *wallet = new_wallet;
        }

        Ok(())
    }

    fn get_transaction(&self, txid: &Txid) -> Result<Transaction, WalletProviderError> {
        let wallet = self.get_wallet()?;

        if let Some(tx) = wallet.get_tx(*txid) {
            Ok((*tx.tx_node.tx).clone())
        } else {
            Err(WalletProviderError::TransactionNotFound(*txid))
        }
    }

    fn get_transactions(&self) -> Result<Vec<Transaction>, WalletProviderError> {
        let wallet = self.get_wallet()?;

        let transactions: Vec<Transaction> = wallet
            .transactions()
            .map(|c_tx| (*c_tx.tx_node.tx).clone())
            .collect();

        Ok(transactions)
    }

    fn get_transaction_by_wallet(
        &self,
        _ids: HashSet<String>,
        txid: &Txid,
    ) -> Result<Transaction, WalletProviderError> {
        // Note: BDK wallet does not support querying transactions by keychain
        self.get_transaction(txid)
    }

    fn get_transactions_by_wallet(
        &self,
        _ids: HashSet<String>,
    ) -> Result<Vec<Transaction>, WalletProviderError> {
        // Note: BDK wallet does not support querying transactions by keychain
        let transactions = self.get_transactions()?;

        Ok(transactions)
    }

    fn get_balance(
        &self,
        ids: HashSet<String>,
        params: super::GetBalanceParams,
    ) -> Result<Amount, WalletProviderError> {
        let wallet = self.get_wallet()?;
        if params.minconf < 1 {
            let balance = self.get_balances(ids)?.total();
            return Ok(balance);
        }

        let checkpoint = wallet.latest_checkpoint();

        let mut balance = Amount::from_sat(0);
        let unspent = wallet.list_unspent();

        let wallet_unspent = unspent.into_iter().filter(|u| {
            !u.is_spent
                && ids.contains(&u.keychain.to_string())
                && u.chain_position
                    .confirmation_height_upper_bound()
                    .is_some_and(|height| {
                        params.minconf
                            <= checkpoint.height().saturating_add(1).saturating_sub(height)
                        // Confirmations = checkpoint_height - (height - 1)
                    })
        });

        for utxo in wallet_unspent {
            balance += utxo.txout.value;
        }

        Ok(balance)
    }

    fn get_balances(&self, ids: HashSet<String>) -> Result<Balance, WalletProviderError> {
        let wallet = self.get_wallet()?;

        let mut immature = Amount::from_sat(0);
        let mut trusted = Amount::from_sat(0);
        let mut untrusted_pending = Amount::from_sat(0);

        for keychain in ids {
            let balance = wallet.balance_keychain(keychain.into());

            immature += balance.immature;
            trusted += balance.trusted_spendable();
            untrusted_pending += balance.untrusted_pending;
        }
        let checkpoint = wallet.latest_checkpoint();
        Ok(Balance {
            immature,
            trusted,
            untrusted_pending,
            used: None, // The BDK wallet does not differentiate used vs unused balance
            last_processed_block: LastProcessedBlock {
                hash: checkpoint.hash(),
                height: checkpoint.height(),
            },
        })
    }

    fn create_transaction(
        &self,
        _ids: HashSet<String>,
        _address: &str,
    ) -> Result<(), WalletProviderError> {
        // let amount_sats = 100_000; // Exemplo: enviar 0.001 BTC
        // let wallet = self.get_wallet()?;

        // // Parsear endereço
        // let address = Address::try_from_unchecked(address)
        //     .map_err(|e| WalletProviderError::Other(format!("Invalid address: {}", e)))?;

        // // Construir transação
        // let mut tx_builder = wallet.build_tx();

        // tx_builder
        //     .add_recipient(address.script_pubkey(), Amount::from_sat(amount_sats))
        //     .map_err(|e| WalletProviderError::Other(format!("Failed to add recipient: {}", e)))?;

        // // Definir taxa
        // tx_builder.fee_rate(bdk_wallet::FeeRate::from_sat_per_vb(5.0));

        // // Finalizar
        // let (mut psbt, details) = tx_builder.finish().map_err(|e| {
        //     WalletProviderError::Other(format!("Failed to build transaction: {}", e))
        // })?;

        // // Assinar PSBT
        // wallet
        //     .sign(&mut psbt, Default::default())
        //     .map_err(|e| WalletProviderError::Other(format!("Failed to sign: {}", e)))?;

        // // Extrair transação assinada
        // let tx = psbt
        //     .extract_tx()
        //     .map_err(|e| WalletProviderError::Other(format!("Failed to extract tx: {}", e)))?;

        // println!("Transação assinada! TXID: {}", tx.compute_txid());
        // Ok(tx)
        Ok(())
    }

    fn new_address(&self, id: &str) -> Result<Address, WalletProviderError> {
        let mut wallet = self.get_wallet_mut()?;
        let keychain_key = K::from(id.to_string());

        // Now keychain is K, no need to match against &str
        let address = wallet
            .next_unused_address(keychain_key)
            .map(|address_info| address_info.address)
            .ok_or_else(|| {
                WalletProviderError::AddressError("No unused address available".to_string())
            })?;

        wallet.persist(&mut *self.get_persister()?).map_err(|_| {
            WalletProviderError::PersistenceError("Persist error wallet".to_string())
        })?;

        Ok(address)
    }

    fn sent_and_received(
        &self,
        ids: HashSet<String>,
        txid: &Txid,
    ) -> Result<(u64, u64), WalletProviderError> {
        let wallet = self.get_wallet()?;

        let tx = self.get_transaction_by_wallet(ids, txid)?;
        let (sent, receive) = wallet.sent_and_received(&tx);

        Ok((sent.to_sat(), receive.to_sat()))
    }

    fn process_mempool_transactions(
        &self,
        transactions: Vec<&Transaction>,
    ) -> Result<Vec<WalletProviderEvent>, WalletProviderError> {
        let mut events = Vec::new();
        let mut unconfirmed_txs = Vec::new();
        let current_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| WalletProviderError::Other(format!("System time error: {e:?}")))?
            .as_secs();

        for tx in &transactions {
            let tx_event = self.get_owned_transaction_outputs(tx)?;
            if !tx_event.is_empty() {
                events.extend(tx_event);
            }
            unconfirmed_txs.push(((*tx).clone(), current_time));
        }

        if unconfirmed_txs.is_empty() {
            return Ok(vec![]);
        }

        let mut wallet = self.get_wallet_mut()?;

        wallet.apply_unconfirmed_txs(unconfirmed_txs);

        wallet.persist(&mut *self.get_persister()?).map_err(|_| {
            WalletProviderError::PersistenceError("Persist error wallet".to_string())
        })?;

        for tx in transactions {
            if wallet.get_tx(tx.compute_txid()).is_some() {
                events
                    .push(WalletProviderEvent::UnconfirmedTransactionInBlock { tx: (*tx).clone() });
            }
        }

        Ok(events)
    }

    fn get_txo(
        &self,
        outpoint: &OutPoint,
        is_spent: Option<bool>,
    ) -> Result<Option<TxOut>, WalletProviderError> {
        let wallet = self.get_wallet()?;

        if let Some(false) = is_spent {
            return Ok(wallet.get_utxo(*outpoint).map(|utxo| utxo.txout.clone()));
        }

        let out = wallet
            .list_output()
            .find(|o| is_spent.is_none_or(|spent| o.is_spent == spent) && o.outpoint == *outpoint);

        Ok(out.map(|o| o.txout.clone()))
    }

    fn get_local_output_by_script(
        &self,
        script_hash: ScriptBuf,
        is_spent: Option<bool>,
    ) -> Result<Vec<LocalOutput>, WalletProviderError> {
        let wallet = self.get_wallet()?;

        let outputs = wallet
            .list_output()
            .filter(|o| {
                is_spent.is_none_or(|spent| o.is_spent == spent)
                    && o.txout.script_pubkey == script_hash
            })
            .map(|o| LocalOutput {
                outpoint: o.outpoint,
                txout: o.txout.clone(),
                is_spent: o.is_spent,
            })
            .collect();

        Ok(outputs)
    }

    fn list_script_buff(
        &self,
        ids: Option<HashSet<String>>,
    ) -> Result<Vec<ScriptBuf>, WalletProviderError> {
        let wallet = self.get_wallet()?;

        let mut script_buf = Vec::new();

        for (id, spk_iter) in wallet.all_unbounded_spk_iters() {
            if let Some(keychains) = &ids {
                if !keychains.contains(&id.to_string()) {
                    continue;
                }
            }
            let index = 30 + wallet.spk_index().last_revealed_index(id).unwrap_or(0);
            let script = spk_iter
                .into_iter()
                .take(index as usize)
                .map(|(_, s)| s)
                .collect::<Vec<_>>();
            script_buf.extend(script);
        }

        Ok(script_buf)
    }

    fn get_last_processed_block(&self) -> Result<LastProcessedBlock, WalletProviderError> {
        let wallet = self.get_wallet()?;

        let checkpoint = wallet.latest_checkpoint();

        Ok(LastProcessedBlock {
            hash: checkpoint.hash(),
            height: checkpoint.height(),
        })
    }

    fn get_descriptor(&self, id: &str) -> Result<String, WalletProviderError> {
        let wallet = self.get_wallet()?;

        let keychain = wallet
            .keyring()
            .list_keychains()
            .get(&K::from(id.to_string()))
            .ok_or_else(|| {
                WalletProviderError::MissingWallet(format!("Keychain with id {id} not found"))
            })?;

        Ok(keychain.to_string())
    }
}

impl From<RusqliteError> for WalletProviderError {
    fn from(value: RusqliteError) -> Self {
        WalletProviderError::PersistenceError(format!("Rusqlite error: {value:?}"))
    }
}

impl<E, K> From<CreateWithPersistError<E, K>> for WalletProviderError
where
    K: Ord + Clone + Debug + From<String>,
{
    fn from(value: CreateWithPersistError<E, K>) -> Self {
        match value {
            CreateWithPersistError::DataAlreadyExists(_) => {
                WalletProviderError::WalletAlreadyExists("Data already exists".to_string())
            }
            CreateWithPersistError::InvalidKeyRing(_) => {
                WalletProviderError::WalletCreationError("Invalid keyring".to_string())
            }
            CreateWithPersistError::Persist(_) => {
                WalletProviderError::PersistenceError("Persist error".to_string())
            }
        }
    }
}

impl<K> From<LoadWithPersistError<RusqliteError, K>> for WalletProviderError
where
    K: Ord + Clone + Debug,
{
    fn from(value: LoadWithPersistError<RusqliteError, K>) -> Self {
        match value {
            LoadWithPersistError::InvalidChangeSet(e) => {
                WalletProviderError::WalletLoadError(format!("Wallet load error: {e:?}"))
            }
            LoadWithPersistError::Persist(e) => {
                WalletProviderError::PersistenceError(format!("Rusqlite error: {e:?}"))
            }
        }
    }
}

impl<K> From<KeyRingError<K>> for WalletProviderError
where
    K: Ord + Clone + Debug + From<String>,
{
    fn from(value: KeyRingError<K>) -> Self {
        match value {
            KeyRingError::DescAlreadyExists(des) => {
                WalletProviderError::DescriptorAlreadyExists(format!("{des:?}"))
            }
            KeyRingError::DescMissing => WalletProviderError::MissingDescriptor,
            KeyRingError::Descriptor(e) => WalletProviderError::InvalidDescriptor(e.to_string()),
            KeyRingError::DescriptorMismatch {
                keychain,
                loaded,
                expected,
            } => WalletProviderError::MismatchedDescriptor(format!(
                "Descriptor mismatch for keychain {keychain:?}: loaded {loaded:?}, expected {expected:?}",
            )),
            KeyRingError::KeychainAlreadyExists(k) => WalletProviderError::WalletError(format!(
                "Invalid label descriptors {k:?} already exists"
            )),
            KeyRingError::NetworkMismatch { loaded, expected } => {
                WalletProviderError::NetworkMismatch {
                    expected,
                    found: loaded,
                }
            }
            KeyRingError::MissingNetwork => WalletProviderError::NetworkMissing,
            KeyRingError::MissingKeychain(k) => WalletProviderError::MissingWallet(format!(
                "Missing label descriptor in wallet: {k:?}"
            )),
        }
    }
}

impl From<CannotConnectError> for WalletProviderError {
    fn from(value: CannotConnectError) -> Self {
        WalletProviderError::BlockProcessingError(value.to_string())
    }
}

#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {

    use bdk_wallet::chain::BlockId;
    use bdk_wallet::chain::ConfirmationBlockTime;

    use super::*;
    use crate::provider::WalletProvider;
    use crate::utils::create_test_transaction;
    use crate::utils::create_transaction_with_txo;

    const DESCRIPTOR: &str = "wpkh(tpubDDtyive2LqLWKzPZ8LZ9Ebi1JDoLcf1cEpn3Mshp6sxVfCupHZJRPQTozp2EpTF76vJcyQBN7VP7CjUntEJxeADnuTMNTYKoSWNae8soVyv/0/*)#7h6kdtnk";
    const DESCRIPTOR_ID: &str = "main";

    const DESCRIPTOR_SECOND: &str = "wpkh(tpubDDtyive2LqLWKzPZ8LZ9Ebi1JDoLcf1cEpn3Mshp6sxVfCupHZJRPQTozp2EpTF76vJcyQBN7VP7CjUntEJxeADnuTMNTYKoSWNae8soVyv/1/*)#0rlhs7rw";
    const DESCRIPTOR_SECOND_ID: &str = "change";

    fn create_test_provider() -> BdkWalletProvider<Connection, KeyId> {
        BdkWalletProvider::<Connection, KeyId>::new_in_memory(Network::Regtest).unwrap()
    }

    fn create_test_provider_initialized() -> BdkWalletProvider<Connection, KeyId> {
        let mut provider = create_test_provider();
        provider
            .persist_descriptor(DESCRIPTOR_ID, DESCRIPTOR)
            .unwrap();

        provider
            .persist_descriptor(DESCRIPTOR_SECOND_ID, DESCRIPTOR_SECOND)
            .unwrap();

        provider
    }

    fn check_descriptor_in_keychain(
        provider: &BdkWalletProvider<Connection, KeyId>,
        id: &str,
        expected_descriptor: &str,
    ) {
        let result = provider.get_descriptor(id).unwrap();

        assert_eq!(
            result, expected_descriptor,
            "Descriptor should match expected value"
        );
    }

    fn create_txo_by_wallet(provider: &BdkWalletProvider<Connection, KeyId>) -> TxOut {
        TxOut {
            value: Amount::from_sat(100_000),
            script_pubkey: provider.new_address(DESCRIPTOR_ID).unwrap().script_pubkey(),
        }
    }

    fn get_test_transaction(
        provider: &BdkWalletProvider<Connection, KeyId>,
        my_output: bool,
    ) -> Transaction {
        if my_output {
            create_transaction_with_txo(create_txo_by_wallet(provider))
        } else {
            create_test_transaction()
        }
    }

    macro_rules! assert_and_pop_event {
        // ConfirmedTransaction
        ($events:expr,ConfirmedTransaction, $expected_tx:expr) => {{
            let event = $events.remove(0);
            if let WalletProviderEvent::ConfirmedTransaction { tx: result_tx } = event {
                assert_eq!(result_tx, $expected_tx);
            } else {
                panic!("Expected ConfirmedTransaction, got {:?}", event);
            }
        }};

        // UnconfirmedTransactionInBlock
        ($events:expr,UnconfirmedTransactionInBlock, $expected_tx:expr) => {{
            let event = $events.remove(0);
            if let WalletProviderEvent::UnconfirmedTransactionInBlock { tx: result_tx } = event {
                assert_eq!(result_tx, $expected_tx);
            } else {
                panic!("Expected UnconfirmedTransactionInBlock, got {:?}", event);
            }
        }};

        // UpdateTransaction
        ($events:expr,UpdateTransaction, $expected_tx:expr, $expected_output:expr) => {{
            let event = $events.remove(0);
            if let WalletProviderEvent::UpdateTransaction {
                tx: result_tx,
                output: result_output,
            } = event
            {
                assert_eq!(result_tx, $expected_tx);
                assert_eq!(result_output, $expected_output);
            } else {
                panic!("Expected UpdateTransaction, got {:?}", event);
            }
        }};
    }

    #[test]
    fn test_get_wallet_not_initialized() {
        let provider = create_test_provider();

        let result = provider.get_wallet();

        assert!(
            result.is_err(),
            "Should fail to get wallet when not initialized"
        );
        assert!(matches!(
            result.unwrap_err(),
            WalletProviderError::WalletNotInitialized
        ));
    }

    #[test]
    fn test_get_wallet_initialized() {
        let provider = create_test_provider_initialized();

        let result = provider.get_wallet();

        assert!(
            result.is_ok(),
            "Should successfully get wallet when initialized"
        );
    }

    #[test]
    fn test_get_wallet_mut_not_initialized() {
        let provider = create_test_provider();

        let result = provider.get_wallet_mut();

        assert!(
            result.is_err(),
            "Should fail to get mutable wallet when not initialized"
        );
        assert!(matches!(
            result.unwrap_err(),
            WalletProviderError::WalletNotInitialized
        ));
    }

    #[test]
    fn test_get_wallet_mut_initialized() {
        let provider = create_test_provider_initialized();

        let result = provider.get_wallet_mut();

        assert!(
            result.is_ok(),
            "Should successfully get mutable wallet when initialized"
        );
    }

    #[test]
    fn test_get_persister() {
        let provider = create_test_provider();

        let result = provider.get_persister();

        assert!(result.is_ok(), "Should successfully get persister");
    }

    #[test]
    fn test_initialize_wallet() {
        let mut provider = create_test_provider();

        let result = provider.initialize_wallet(DESCRIPTOR_ID, DESCRIPTOR);

        assert!(result.is_ok(), "Should successfully initialize wallet");
        assert!(
            provider.wallet.is_some(),
            "Wallet should be set after initialization"
        );

        // Verify the descriptor is in the keychain
        check_descriptor_in_keychain(&provider, DESCRIPTOR_ID, DESCRIPTOR);
    }

    #[test]
    fn test_event_process_confirmed_transaction_without_my_output() {
        let provider = create_test_provider_initialized();

        // Create a simple transaction
        let tx = get_test_transaction(&provider, false);
        let txid = tx.compute_txid();

        let event = WalletEvent::TxConfirmed {
            txid,
            tx: Arc::new(tx.clone()),
            block_time: ConfirmationBlockTime::default(),
            old_block_time: None,
        };

        let mut result = provider.event_process(vec![event]).unwrap();

        assert_eq!(result.len(), 1);
        assert_and_pop_event!(result, ConfirmedTransaction, tx);
        assert!(result.is_empty(), "No more events should be generated");
    }

    #[test]
    fn test_event_process_confirmed_transaction_with_my_output() {
        let provider = create_test_provider_initialized();

        // Create a transaction with our output
        let tx = get_test_transaction(&provider, true);
        let txid = tx.compute_txid();

        let event = WalletEvent::TxConfirmed {
            txid,
            tx: Arc::new(tx.clone()),
            block_time: ConfirmationBlockTime::default(),
            old_block_time: None,
        };

        let mut result = provider.event_process(vec![event]).unwrap();

        assert_eq!(result.len(), 2);
        assert_and_pop_event!(
            result,
            UpdateTransaction,
            tx,
            tx.output[tx.output.len() - 1].clone()
        );
        assert_and_pop_event!(result, ConfirmedTransaction, tx);

        assert!(result.is_empty(), "No more events should be generated");
    }

    #[test]
    fn test_event_process_unconfirmed_transaction_without_my_output() {
        let provider = create_test_provider_initialized();

        let tx = get_test_transaction(&provider, false);
        let txid = tx.compute_txid();

        let event = WalletEvent::TxUnconfirmed {
            txid,
            tx: Arc::new(tx.clone()),
            old_block_time: None,
        };

        let mut result = provider.event_process(vec![event]).unwrap();

        assert_eq!(result.len(), 1);
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx);
        assert!(result.is_empty(), "No more events should be generated");
    }

    #[test]
    fn test_event_process_unconfirmed_transaction_with_my_output() {
        let provider = create_test_provider_initialized();

        let tx = get_test_transaction(&provider, true);
        let txid = tx.compute_txid();

        let event = WalletEvent::TxUnconfirmed {
            txid,
            tx: Arc::new(tx.clone()),
            old_block_time: None,
        };

        let mut result = provider.event_process(vec![event]).unwrap();

        assert_eq!(result.len(), 2);
        assert_and_pop_event!(
            result,
            UpdateTransaction,
            tx,
            tx.output[tx.output.len() - 1].clone()
        );
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx);
        assert!(result.is_empty(), "No more events should be generated");
    }

    #[test]
    fn test_event_process_drop_transaction_without_my_output() {
        let provider = create_test_provider_initialized();

        let tx = get_test_transaction(&provider, false);
        let txid = tx.compute_txid();

        let event = WalletEvent::TxDropped {
            txid,
            tx: Arc::new(tx.clone()),
        };

        let mut result = provider.event_process([event].to_vec()).unwrap();

        assert_eq!(result.len(), 1);
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx);
        assert!(result.is_empty(), "No more events should be generated");
    }

    #[test]
    fn test_event_process_drop_transaction_with_my_output() {
        let provider = create_test_provider_initialized();

        let tx = get_test_transaction(&provider, true);
        let txid = tx.compute_txid();

        let event = WalletEvent::TxDropped {
            txid,
            tx: Arc::new(tx.clone()),
        };

        let mut result = provider.event_process(vec![event]).unwrap();

        assert_eq!(result.len(), 2);
        assert_and_pop_event!(
            result,
            UpdateTransaction,
            tx,
            tx.output[tx.output.len() - 1].clone()
        );
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx);
        assert!(result.is_empty(), "No more events should be generated");
    }

    #[test]
    fn test_event_process_replaced_transaction_without_my_output() {
        let provider = create_test_provider_initialized();

        let tx = get_test_transaction(&provider, false);
        let txid = tx.compute_txid();

        let event = WalletEvent::TxReplaced {
            txid,
            tx: Arc::new(tx.clone()),
            conflicts: vec![],
        };

        let mut result = provider.event_process(vec![event]).unwrap();

        assert_eq!(result.len(), 1);
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx);
        assert!(result.is_empty(), "No more events should be generated");
    }

    #[test]
    fn test_event_process_replaced_transaction_with_my_output() {
        let provider = create_test_provider_initialized();

        let tx = get_test_transaction(&provider, true);
        let txid = tx.compute_txid();

        let event = WalletEvent::TxReplaced {
            txid,
            tx: Arc::new(tx.clone()),
            conflicts: vec![],
        };

        let mut result = provider.event_process(vec![event]).unwrap();

        assert_eq!(result.len(), 2);
        assert_and_pop_event!(
            result,
            UpdateTransaction,
            tx,
            tx.output[tx.output.len() - 1].clone()
        );
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx);
        assert!(result.is_empty(), "No more events should be generated");
    }

    #[test]
    fn test_event_process_chain_tip_changed() {
        let provider = create_test_provider_initialized();

        let event = WalletEvent::ChainTipChanged {
            old_tip: BlockId::default(),
            new_tip: BlockId::default(),
        };

        let result = provider.event_process(vec![event]).unwrap();

        assert!(
            result.is_empty(),
            "ChainTipChanged should not generate any events"
        );
    }

    #[test]
    fn test_get_owned_transaction_outputs_empty() {
        let provider = create_test_provider_initialized();

        let tx = get_test_transaction(&provider, true);

        let result = provider.get_owned_transaction_outputs(&tx).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0],
            WalletProviderEvent::UpdateTransaction {
                tx: tx.clone(),
                output: tx.output[tx.output.len() - 1].clone()
            }
        );
    }

    #[test]
    fn test_get_owned_transaction_outputs_with_outputs() {
        let provider = create_test_provider_initialized();

        let tx = get_test_transaction(&provider, true);

        let result = provider.get_owned_transaction_outputs(&tx).unwrap();
        assert!(
            !result.is_empty(),
            "Transaction with our outputs should generate events"
        );
    }

    #[test]
    fn test_event_process_multiple_events() {
        let provider = create_test_provider_initialized();

        // === Phase 1: Transactions WITH user output ===
        let tx_with_output = get_test_transaction(&provider, true);
        let tx_with_output_id = tx_with_output.compute_txid();

        // === Phase 2: Transactions WITHOUT user output ===
        let tx_without_output = get_test_transaction(&provider, false);
        let tx_without_output_id = tx_without_output.compute_txid();

        // Build events: first all WITH user output, then all WITHOUT user output
        let events = vec![
            // --- Events WITH user output (should generate 2 events each) ---
            WalletEvent::TxConfirmed {
                txid: tx_with_output_id,
                tx: Arc::new(tx_with_output.clone()),
                block_time: ConfirmationBlockTime::default(),
                old_block_time: None,
            },
            WalletEvent::TxUnconfirmed {
                txid: tx_with_output_id,
                tx: Arc::new(tx_with_output.clone()),
                old_block_time: None,
            },
            WalletEvent::TxDropped {
                txid: tx_with_output_id,
                tx: Arc::new(tx_with_output.clone()),
            },
            WalletEvent::TxReplaced {
                txid: tx_with_output_id,
                tx: Arc::new(tx_with_output.clone()),
                conflicts: vec![],
            },
            // --- Events WITHOUT user output (should generate 1 event each) ---
            WalletEvent::TxConfirmed {
                txid: tx_without_output_id,
                tx: Arc::new(tx_without_output.clone()),
                block_time: ConfirmationBlockTime::default(),
                old_block_time: None,
            },
            WalletEvent::TxUnconfirmed {
                txid: tx_without_output_id,
                tx: Arc::new(tx_without_output.clone()),
                old_block_time: None,
            },
            WalletEvent::TxDropped {
                txid: tx_without_output_id,
                tx: Arc::new(tx_without_output.clone()),
            },
            WalletEvent::TxReplaced {
                txid: tx_without_output_id,
                tx: Arc::new(tx_without_output.clone()),
                conflicts: vec![],
            },
            WalletEvent::ChainTipChanged {
                old_tip: BlockId::default(),
                new_tip: BlockId::default(),
            },
        ];

        let mut result = provider.event_process(events).unwrap();

        // === Validate: TxConfirmed WITH output (2 events) ===
        assert_and_pop_event!(
            result,
            UpdateTransaction,
            tx_with_output,
            tx_with_output.output[tx_with_output.output.len() - 1].clone()
        );
        assert_and_pop_event!(result, ConfirmedTransaction, tx_with_output);

        // === Validate: TxUnconfirmed WITH output (2 events) ===
        assert_and_pop_event!(
            result,
            UpdateTransaction,
            tx_with_output,
            tx_with_output.output[tx_with_output.output.len() - 1].clone()
        );
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx_with_output);

        // === Validate: TxDropped WITH output (2 events) ===
        assert_and_pop_event!(
            result,
            UpdateTransaction,
            tx_with_output,
            tx_with_output.output[tx_with_output.output.len() - 1].clone()
        );
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx_with_output);

        // === Validate: TxReplaced WITH output (2 events) ===
        assert_and_pop_event!(
            result,
            UpdateTransaction,
            tx_with_output,
            tx_with_output.output[tx_with_output.output.len() - 1].clone()
        );
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx_with_output);

        // === Validate: TxConfirmed WITHOUT output (1 event) ===
        assert_and_pop_event!(result, ConfirmedTransaction, tx_without_output);

        // === Validate: TxUnconfirmed WITHOUT output (1 event) ===
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx_without_output);

        // === Validate: TxDropped WITHOUT output (1 event) ===
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx_without_output);

        // === Validate: TxReplaced WITHOUT output (1 event) ===
        assert_and_pop_event!(result, UnconfirmedTransactionInBlock, tx_without_output);

        // === Validate: ChainTipChanged (0 events) ===
        // Already validated if we reach this point with empty result
        assert!(
            result.is_empty(),
            "All events should have been processed correctly"
        );
    }
}
