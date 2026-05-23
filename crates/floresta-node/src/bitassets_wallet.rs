use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{SocketAddrV4, SocketAddrV6};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bitcoin::base58;
use bitcoin::bip32::{ChildNumber, DerivationPath, Xpriv};
use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::Signer as _;
use rand::RngCore as _;
use rustreexo::{node_hash::BitcoinNodeHash, proof::Proof, stump::Stump};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid BitAssets address")]
    Address(#[from] bitcoin::base58::InvalidCharacterError),
    #[error("invalid BitAssets address length {0} != 20")]
    AddressLength(usize),
    #[error("BitAssets native wallet requires --bitassets-wallet-create or --bitassets-wallet-seed for first startup")]
    CreateRequired,
    #[error("bip32 error")]
    Bip32(#[from] bitcoin::bip32::Error),
    #[error("ed25519 error")]
    Ed25519(#[from] ed25519_dalek::SignatureError),
    #[error("hex decode error")]
    Hex(#[from] hex::FromHexError),
    #[error("io error")]
    Io(#[from] std::io::Error),
    #[error("not enough native wallet BitAsset funds for {asset_id}: need {amount}")]
    NotEnoughFunds { asset_id: String, amount: u64 },
    #[error("native BitAssets wallet has no wallet UTXO matching {0}")]
    NoWalletUtxo(String),
    #[error("BitAssets native wallet has no address for input {0}")]
    NoSigningAddress(String),
    #[error("JSON error")]
    Json(#[from] serde_json::Error),
    #[error("BitAssets RPC error: {0}")]
    Rpc(String),
    #[error("native wallet seed must be exactly 64 bytes, got {0}")]
    SeedLength(usize),
    #[error("invalid native wallet BitAssets Utreexo proof for {0}")]
    UtreexoProof(String),
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct Address([u8; 20]);

impl Address {
    fn from_base58(s: &str) -> Result<Self, Error> {
        let decoded = base58::decode(s)?;
        let len = decoded.len();
        let bytes = decoded
            .try_into()
            .map_err(|_: Vec<u8>| Error::AddressLength(len))?;
        Ok(Self(bytes))
    }

    fn as_base58(&self) -> String {
        base58::encode(&self.0)
    }

    fn script_hash(&self) -> String {
        hex::encode(blake3::hash(&self.0).as_bytes())
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Hash([u8; 32]);

impl Hash {
    fn from_hex(s: &str) -> Result<Self, Error> {
        Ok(Self(hex::decode(s)?.try_into().map_err(
            |bytes: Vec<u8>| {
                Error::Rpc(format!("expected 32-byte hash, got {} bytes", bytes.len()))
            },
        )?))
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as Deserialize>::deserialize(deserializer)?;
        Self::from_hex(&value).map_err(serde::de::Error::custom)
    }
}

impl Serialize for Hash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&hex::encode(self.0))
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct Txid(Hash);

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct MerkleRoot(Hash);

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
enum OutPoint {
    Regular { txid: Txid, vout: u32 },
    Coinbase { merkle_root: MerkleRoot, vout: u32 },
    Deposit(BitcoinOutPoint),
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
struct BitcoinOutPoint {
    txid: [u8; 32],
    vout: u32,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Eq, PartialEq)]
enum OutputContent {
    AmmLpToken(u64),
    Bitcoin(u64),
    BitAsset(u64),
    BitAssetControl,
    BitAssetReservation,
    DutchAuctionReceipt,
    Withdrawal(WithdrawalContent),
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Eq, PartialEq)]
struct WithdrawalContent {
    value: u64,
    main_fee: u64,
    main_address_script: Vec<u8>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Eq, PartialEq)]
struct Output {
    address: Address,
    content: OutputContent,
    memo: Vec<u8>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Eq, PartialEq)]
enum TransactionData {
    AmmBurn {
        amount0: u64,
        amount1: u64,
        lp_token_burn: u64,
    },
    AmmMint {
        amount0: u64,
        amount1: u64,
        lp_token_mint: u64,
    },
    AmmSwap {
        amount_spent: u64,
        amount_receive: u64,
        pair_asset: AssetId,
    },
    BitAssetReservation {
        commitment: Hash,
    },
    BitAssetRegistration {
        name_hash: Hash,
        revealed_nonce: Hash,
        bitasset_data: Box<BitAssetData>,
        initial_supply: u64,
    },
    BitAssetMint(u64),
    BitAssetUpdate(Box<BitAssetDataUpdates>),
    DutchAuctionCreate(DutchAuctionParams),
    DutchAuctionBid {
        auction_id: DutchAuctionId,
        receive_asset: AssetId,
        quantity: u64,
        bid_size: u64,
    },
    DutchAuctionCollect {
        asset_offered: AssetId,
        asset_receive: AssetId,
        amount_offered_remaining: u64,
        amount_received: u64,
    },
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetId {
    Bitcoin,
    BitAsset(BitAssetId),
    BitAssetControl(BitAssetId),
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct BitAssetId(pub Hash);

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct DutchAuctionId(Txid);

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Eq, PartialEq)]
pub struct DutchAuctionParams {
    pub start_block: u32,
    pub duration: u32,
    pub base_asset: AssetId,
    pub base_amount: u64,
    pub quote_asset: AssetId,
    pub initial_price: u64,
    pub final_price: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct EncryptionPubKey([u8; 32]);

impl<'de> Deserialize<'de> for EncryptionPubKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as Deserialize>::deserialize(deserializer)?;
        let bytes = hex::decode(&value).map_err(serde::de::Error::custom)?;
        let len = bytes.len();
        let bytes = bytes.try_into().map_err(|_: Vec<u8>| {
            serde::de::Error::custom(format!("expected 32-byte encryption pubkey, got {len}"))
        })?;
        Ok(Self(bytes))
    }
}

impl Serialize for EncryptionPubKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&hex::encode(self.0))
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BitAssetData {
    pub commitment: Option<Hash>,
    pub socket_addr_v4: Option<SocketAddrV4>,
    pub socket_addr_v6: Option<SocketAddrV6>,
    pub encryption_pubkey: Option<EncryptionPubKey>,
    pub signing_pubkey: Option<VerifyingKey>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Eq, PartialEq)]
struct BitAssetDataUpdates {
    ticker: Option<Option<String>>,
    name: Option<Option<String>>,
    summary: Option<Option<String>>,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Eq, PartialEq)]
struct Transaction {
    inputs: Vec<OutPoint>,
    outputs: Vec<Output>,
    memo: Vec<u8>,
    data: Option<TransactionData>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct VerifyingKey(ed25519_dalek::VerifyingKey);

impl BorshSerialize for VerifyingKey {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&self.0.to_bytes(), writer)
    }
}

impl BorshDeserialize for VerifyingKey {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let bytes = <[u8; ed25519_dalek::PUBLIC_KEY_LENGTH]>::deserialize_reader(reader)?;
        ed25519_dalek::VerifyingKey::from_bytes(&bytes)
            .map(Self)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
    }
}

impl<'de> Deserialize<'de> for VerifyingKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as Deserialize>::deserialize(deserializer)?;
        let bytes = hex::decode(&value).map_err(serde::de::Error::custom)?;
        let len = bytes.len();
        let bytes = bytes.try_into().map_err(|_: Vec<u8>| {
            serde::de::Error::custom(format!("expected 32-byte verifying key, got {len}"))
        })?;
        ed25519_dalek::VerifyingKey::from_bytes(&bytes)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

impl Serialize for VerifyingKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&hex::encode(self.0.to_bytes()))
    }
}

impl VerifyingKey {
    fn address(&self) -> Address {
        let mut reader = blake3::Hasher::new()
            .update(&self.0.to_bytes())
            .finalize_xof();
        let mut output = [0u8; 20];
        reader.fill(&mut output);
        Address(output)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct Signature(ed25519_dalek::Signature);

impl BorshSerialize for Signature {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&self.0.to_bytes(), writer)
    }
}

impl BorshDeserialize for Signature {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let bytes = <[u8; ed25519_dalek::Signature::BYTE_SIZE]>::deserialize_reader(reader)?;
        Ok(Self(ed25519_dalek::Signature::from_bytes(&bytes)))
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
struct Authorization {
    verifying_key: VerifyingKey,
    signature: Signature,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Eq, PartialEq)]
struct AuthorizedTransaction {
    transaction: Transaction,
    authorizations: Vec<Authorization>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredAddress {
    index: u32,
    address: String,
    #[serde(default)]
    script_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct WalletOutPoint {
    pub txid: String,
    pub vout: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletUtxo {
    pub outpoint: WalletOutPoint,
    pub address: String,
    pub asset_id: String,
    pub amount: u64,
    pub confirmed: bool,
    pub proof_refs: Vec<WalletProofRef>,
    #[serde(default)]
    pub utreexo_leaf_hash: Option<String>,
    #[serde(default = "default_content_kind")]
    pub content_kind: String,
    #[serde(default)]
    pub reservation_txid: Option<String>,
    #[serde(default)]
    pub reservation_commitment: Option<String>,
    #[serde(default)]
    pub lp_asset0: Option<String>,
    #[serde(default)]
    pub lp_asset1: Option<String>,
    #[serde(default)]
    pub auction_id: Option<String>,
}

fn default_content_kind() -> String {
    "bitasset".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WalletProofRef {
    pub txid: String,
    pub block_hash: Option<String>,
    pub sidechain_block_height: Option<u32>,
    pub bmm_inclusions: Vec<String>,
    pub best_main_verification: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredWallet {
    version: u32,
    seed_hex: String,
    next_index: u32,
    addresses: Vec<StoredAddress>,
    confirmed_utxos: Vec<WalletUtxo>,
    mempool_utxos: Vec<WalletUtxo>,
    spent_outpoints: Vec<WalletOutPoint>,
    last_tip_hash: Option<String>,
    last_tip_height: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct QuicStatus {
    pub enabled: bool,
    pub connected: bool,
    pub last_message_unix_ms: Option<u128>,
    pub last_error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NativeBitAssetsWallet {
    path: PathBuf,
    rpc_url: String,
    stored: StoredWallet,
    runtime_seed_hex: Option<String>,
    persist_seed: bool,
    quic_status: QuicStatus,
    subscription_generation: u64,
}

const BITASSETS_RPC_TIMEOUT: Duration = Duration::from_secs(30);

impl NativeBitAssetsWallet {
    pub fn open(
        path: impl AsRef<Path>,
        rpc_url: String,
        seed_hex: Option<&str>,
        create: bool,
    ) -> Result<Self, Error> {
        Self::open_with_seed_storage(path, rpc_url, seed_hex, create, true)
    }

    pub fn open_with_seed_storage(
        path: impl AsRef<Path>,
        rpc_url: String,
        seed_hex: Option<&str>,
        create: bool,
        persist_seed: bool,
    ) -> Result<Self, Error> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            let bytes = fs::read(&path)?;
            let mut stored: StoredWallet = serde_json::from_slice(&bytes)?;
            if stored.seed_hex.is_empty() && seed_hex.is_none() {
                return Err(Error::CreateRequired);
            }
            let runtime_seed_hex = seed_hex.map(ToOwned::to_owned);
            if persist_seed {
                if let Some(seed_hex) = seed_hex {
                    stored.seed_hex = seed_hex.to_string();
                }
            } else {
                stored.seed_hex.clear();
            }
            for address in &mut stored.addresses {
                if address.script_hash.is_none() {
                    address.script_hash =
                        Some(Address::from_base58(&address.address)?.script_hash());
                }
            }
            let wallet = Self {
                path,
                rpc_url,
                stored,
                runtime_seed_hex,
                persist_seed,
                quic_status: QuicStatus::default(),
                subscription_generation: 0,
            };
            wallet.save()?;
            return Ok(wallet);
        }

        if !create && seed_hex.is_none() {
            return Err(Error::CreateRequired);
        }

        let seed = match seed_hex {
            Some(seed_hex) => decode_seed(seed_hex)?,
            None => {
                let mut seed = [0u8; 64];
                rand::rngs::OsRng.fill_bytes(&mut seed);
                seed
            }
        };
        let wallet = Self {
            path,
            rpc_url,
            stored: StoredWallet {
                version: 1,
                seed_hex: if persist_seed {
                    hex::encode(seed)
                } else {
                    String::new()
                },
                next_index: 0,
                addresses: Vec::new(),
                confirmed_utxos: Vec::new(),
                mempool_utxos: Vec::new(),
                spent_outpoints: Vec::new(),
                last_tip_hash: None,
                last_tip_height: None,
            },
            runtime_seed_hex: if persist_seed {
                None
            } else {
                Some(hex::encode(seed))
            },
            persist_seed,
            quic_status: QuicStatus::default(),
            subscription_generation: 0,
        };
        wallet.save()?;
        Ok(wallet)
    }

    pub fn get_new_address(&mut self) -> Result<String, Error> {
        let index = self.stored.next_index;
        let signing_key = self.signing_key(index)?;
        let address = VerifyingKey(signing_key.verifying_key()).address();
        let address_string = address.as_base58();
        self.stored.next_index = self
            .stored
            .next_index
            .checked_add(1)
            .ok_or_else(|| Error::Rpc("address index overflow".to_string()))?;
        self.stored.addresses.push(StoredAddress {
            index,
            address: address_string.clone(),
            script_hash: Some(address.script_hash()),
        });
        self.save()?;
        self.subscription_generation = self.subscription_generation.saturating_add(1);
        Ok(address_string)
    }

    pub fn wallet_info(&self) -> Value {
        json!({
            "enabled": true,
            "address_count": self.stored.addresses.len(),
            "confirmed_utxo_count": self.stored.confirmed_utxos.len(),
            "mempool_utxo_count": self.stored.mempool_utxos.len(),
            "last_tip_hash": self.stored.last_tip_hash,
            "last_tip_height": self.stored.last_tip_height,
            "balances": self.balances(),
            "quic": self.quic_status,
        })
    }

    pub fn script_hashes(&self) -> Result<Vec<String>, Error> {
        self.stored
            .addresses
            .iter()
            .map(|address| {
                address.script_hash.clone().map(Ok).unwrap_or_else(|| {
                    Address::from_base58(&address.address).map(|a| a.script_hash())
                })
            })
            .collect()
    }

    pub fn last_tip_hash(&self) -> Option<String> {
        self.stored.last_tip_hash.clone()
    }

    pub fn subscription_generation(&self) -> u64 {
        self.subscription_generation
    }

    pub fn set_quic_enabled(&mut self, enabled: bool) {
        self.quic_status.enabled = enabled;
    }

    pub fn set_quic_connected(&mut self, connected: bool) {
        self.quic_status.connected = connected;
        if connected {
            self.quic_status.last_error = None;
            self.quic_status.last_message_unix_ms = unix_ms_now();
        }
    }

    pub fn set_quic_error(&mut self, error: impl Into<String>) {
        self.quic_status.connected = false;
        self.quic_status.last_error = Some(error.into());
    }

    pub fn list_utxos(&self) -> Value {
        json!({
            "confirmed": self.stored.confirmed_utxos,
            "mempool": self.stored.mempool_utxos,
        })
    }

    pub fn get_balance(&self, asset_id: Option<&str>) -> Value {
        let balances = self.balances();
        match asset_id {
            Some(asset_id) => json!({
                "asset_id": asset_id,
                "confirmed": balances.get(asset_id).copied().unwrap_or(0),
            }),
            None => json!(balances),
        }
    }

    pub fn sync(&mut self) -> Result<Value, Error> {
        let script_hashes = self.script_hashes()?;
        if script_hashes.is_empty() {
            return Ok(self.wallet_info());
        }
        let update = bitassets_rpc_call_with_params(
            &self.rpc_url,
            "get_lite_wallet_update",
            vec![json!(script_hashes), json!(self.stored.last_tip_hash)],
        )?;
        self.apply_update(&update)?;
        self.save()?;
        Ok(self.wallet_info())
    }

    pub fn apply_quic_update(&mut self, update: &Value) -> Result<Value, Error> {
        self.apply_update(update)?;
        self.quic_status.connected = true;
        self.quic_status.last_error = None;
        self.quic_status.last_message_unix_ms = unix_ms_now();
        self.save()?;
        Ok(self.wallet_info())
    }

    pub fn transfer(
        &mut self,
        destination: &str,
        asset_id_hex: &str,
        amount: u64,
        fee_sats: u64,
        memo: Option<Vec<u8>>,
    ) -> Result<Value, Error> {
        if fee_sats != 0 {
            return Err(Error::Rpc(
                "native BitAsset transfer currently supports fee_sats=0".to_string(),
            ));
        }
        let asset = AssetId::BitAsset(BitAssetId(Hash::from_hex(asset_id_hex)?));
        let (total, selected) = self.select_wallet_utxos(&asset, amount)?;

        let mut outputs = vec![Output {
            address: Address::from_base58(destination)?,
            content: OutputContent::BitAsset(amount),
            memo: memo.unwrap_or_default(),
        }];
        let change = total - amount;
        if change != 0 {
            let change_address = Address::from_base58(&self.get_new_address()?)?;
            outputs.push(Output {
                address: change_address,
                content: OutputContent::BitAsset(change),
                memo: Vec::new(),
            });
        }

        let transaction = Transaction {
            inputs: selected_to_outpoints(&selected)?,
            outputs,
            memo: Vec::new(),
            data: None,
        };
        let txid = self.sign_and_broadcast(transaction, &selected)?;
        Ok(json!({
            "txid": txid,
            "status": "broadcast",
            "native": true
        }))
    }

    pub fn reserve(&mut self, name: &str, fee_sats: u64) -> Result<Value, Error> {
        reject_nonzero_fee(fee_sats)?;
        let reservation_address = Address::from_base58(&self.get_new_address()?)?;
        let index = self.index_for_address(&reservation_address.as_base58())?;
        let signing_key = self.signing_key(index)?;
        let name_hash = Hash(*blake3::hash(name.as_bytes()).as_bytes());
        let nonce = blake3::keyed_hash(signing_key.as_bytes(), &name_hash.0);
        let commitment = Hash(*blake3::keyed_hash(nonce.as_bytes(), &name_hash.0).as_bytes());
        let transaction = Transaction {
            inputs: Vec::new(),
            outputs: vec![Output {
                address: reservation_address,
                content: OutputContent::BitAssetReservation,
                memo: Vec::new(),
            }],
            memo: Vec::new(),
            data: Some(TransactionData::BitAssetReservation { commitment }),
        };
        let txid = self.sign_and_broadcast(transaction, &[])?;
        Ok(json!({
            "txid": txid,
            "status": "broadcast",
            "native": true,
            "commitment": hex::encode(commitment.0),
        }))
    }

    pub fn register(
        &mut self,
        name: &str,
        initial_supply: u64,
        bitasset_data: BitAssetData,
        fee_sats: u64,
    ) -> Result<Value, Error> {
        reject_nonzero_fee(fee_sats)?;
        let name_hash = Hash(*blake3::hash(name.as_bytes()).as_bytes());
        let bitasset_id = BitAssetId(name_hash);
        let mut selected = None;
        let mut revealed_nonce = None;
        for utxo in self
            .stored
            .confirmed_utxos
            .iter()
            .filter(|utxo| utxo.content_kind == "reservation")
        {
            let Some(commitment) = utxo.reservation_commitment.as_deref() else {
                continue;
            };
            let index = self.index_for_address(&utxo.address)?;
            let signing_key = self.signing_key(index)?;
            let nonce = blake3::keyed_hash(signing_key.as_bytes(), &name_hash.0);
            let computed = blake3::keyed_hash(nonce.as_bytes(), &name_hash.0);
            if hex::encode(computed.as_bytes()) == commitment {
                selected = Some(utxo.clone());
                revealed_nonce = Some(Hash(*nonce.as_bytes()));
                break;
            }
        }
        let selected =
            selected.ok_or_else(|| Error::NoWalletUtxo(format!("reservation for {name}")))?;
        let revealed_nonce = revealed_nonce.expect("set with selected reservation");
        let address = Address::from_base58(&self.get_new_address()?)?;
        let mut outputs = Vec::new();
        if initial_supply != 0 {
            outputs.push(Output {
                address,
                content: OutputContent::BitAsset(initial_supply),
                memo: Vec::new(),
            });
        }
        outputs.push(Output {
            address,
            content: OutputContent::BitAssetControl,
            memo: Vec::new(),
        });
        let transaction = Transaction {
            inputs: selected_to_outpoints(std::slice::from_ref(&selected))?,
            outputs,
            memo: Vec::new(),
            data: Some(TransactionData::BitAssetRegistration {
                name_hash,
                revealed_nonce,
                bitasset_data: Box::new(bitasset_data),
                initial_supply,
            }),
        };
        let txid = self.sign_and_broadcast(transaction, &[selected])?;
        Ok(json!({
            "txid": txid,
            "status": "broadcast",
            "native": true,
            "asset_id": hex::encode(bitasset_id.0.0),
        }))
    }

    pub fn amm_mint(
        &mut self,
        asset0: &str,
        asset1: &str,
        amount0: u64,
        amount1: u64,
        lp_token_mint: u64,
        fee_sats: u64,
    ) -> Result<Value, Error> {
        reject_nonzero_fee(fee_sats)?;
        let asset0 = parse_asset_id(asset0)?;
        let asset1 = parse_asset_id(asset1)?;
        let (input0, mut selected) = self.select_wallet_utxos(&asset0, amount0)?;
        let (input1, selected1) = self.select_wallet_utxos(&asset1, amount1)?;
        selected.extend(selected1);
        let mut outputs = Vec::new();
        self.push_change(&mut outputs, asset0, input0 - amount0)?;
        self.push_change(&mut outputs, asset1, input1 - amount1)?;
        outputs.push(Output {
            address: Address::from_base58(&self.get_new_address()?)?,
            content: OutputContent::AmmLpToken(lp_token_mint),
            memo: Vec::new(),
        });
        self.broadcast_constructor(
            selected,
            outputs,
            TransactionData::AmmMint {
                amount0,
                amount1,
                lp_token_mint,
            },
        )
    }

    pub fn amm_swap(
        &mut self,
        asset_spend: &str,
        asset_receive: &str,
        amount_spend: u64,
        amount_receive: u64,
        fee_sats: u64,
    ) -> Result<Value, Error> {
        reject_nonzero_fee(fee_sats)?;
        let asset_spend = parse_asset_id(asset_spend)?;
        let asset_receive = parse_asset_id(asset_receive)?;
        let (input, selected) = self.select_wallet_utxos(&asset_spend, amount_spend)?;
        let mut outputs = Vec::new();
        self.push_change(&mut outputs, asset_spend, input - amount_spend)?;
        outputs.push(Output {
            address: Address::from_base58(&self.get_new_address()?)?,
            content: output_content_for_asset(asset_receive, amount_receive),
            memo: Vec::new(),
        });
        self.broadcast_constructor(
            selected,
            outputs,
            TransactionData::AmmSwap {
                amount_spent: amount_spend,
                amount_receive,
                pair_asset: asset_receive,
            },
        )
    }

    pub fn amm_burn(
        &mut self,
        asset0: &str,
        asset1: &str,
        amount0: u64,
        amount1: u64,
        lp_token_burn: u64,
        fee_sats: u64,
    ) -> Result<Value, Error> {
        reject_nonzero_fee(fee_sats)?;
        let asset0 = parse_asset_id(asset0)?;
        let asset1 = parse_asset_id(asset1)?;
        let (input, selected) =
            self.select_lp_utxos(&asset_key(&asset0), &asset_key(&asset1), lp_token_burn)?;
        let mut outputs = Vec::new();
        if input != lp_token_burn {
            outputs.push(Output {
                address: Address::from_base58(&self.get_new_address()?)?,
                content: OutputContent::AmmLpToken(input - lp_token_burn),
                memo: Vec::new(),
            });
        }
        outputs.push(Output {
            address: Address::from_base58(&self.get_new_address()?)?,
            content: output_content_for_asset(asset0, amount0),
            memo: Vec::new(),
        });
        outputs.push(Output {
            address: Address::from_base58(&self.get_new_address()?)?,
            content: output_content_for_asset(asset1, amount1),
            memo: Vec::new(),
        });
        self.broadcast_constructor(
            selected,
            outputs,
            TransactionData::AmmBurn {
                amount0,
                amount1,
                lp_token_burn,
            },
        )
    }

    pub fn dutch_auction_create(
        &mut self,
        params: DutchAuctionParams,
        fee_sats: u64,
    ) -> Result<Value, Error> {
        reject_nonzero_fee(fee_sats)?;
        let (input, selected) = self.select_wallet_utxos(&params.base_asset, params.base_amount)?;
        let mut outputs = Vec::new();
        self.push_change(&mut outputs, params.base_asset, input - params.base_amount)?;
        outputs.push(Output {
            address: Address::from_base58(&self.get_new_address()?)?,
            content: OutputContent::DutchAuctionReceipt,
            memo: Vec::new(),
        });
        self.broadcast_constructor(
            selected,
            outputs,
            TransactionData::DutchAuctionCreate(params),
        )
    }

    pub fn dutch_auction_bid(
        &mut self,
        auction_id: &str,
        base_asset: &str,
        quote_asset: &str,
        bid_size: u64,
        receive_quantity: u64,
        fee_sats: u64,
    ) -> Result<Value, Error> {
        reject_nonzero_fee(fee_sats)?;
        let auction_id = DutchAuctionId(Txid(Hash::from_hex(auction_id)?));
        let base_asset = parse_asset_id(base_asset)?;
        let quote_asset = parse_asset_id(quote_asset)?;
        let (input, selected) = self.select_wallet_utxos(&quote_asset, bid_size)?;
        let mut outputs = Vec::new();
        self.push_change(&mut outputs, quote_asset, input - bid_size)?;
        outputs.push(Output {
            address: Address::from_base58(&self.get_new_address()?)?,
            content: output_content_for_asset(base_asset, receive_quantity),
            memo: Vec::new(),
        });
        self.broadcast_constructor(
            selected,
            outputs,
            TransactionData::DutchAuctionBid {
                auction_id,
                receive_asset: base_asset,
                quantity: receive_quantity,
                bid_size,
            },
        )
    }

    pub fn dutch_auction_collect(
        &mut self,
        auction_id: &str,
        base_asset: &str,
        quote_asset: &str,
        amount_base: u64,
        amount_quote: u64,
        fee_sats: u64,
    ) -> Result<Value, Error> {
        reject_nonzero_fee(fee_sats)?;
        let base_asset = parse_asset_id(base_asset)?;
        let quote_asset = parse_asset_id(quote_asset)?;
        let selected = self.select_auction_receipt(auction_id)?;
        let mut outputs = Vec::new();
        if amount_base != 0 {
            outputs.push(Output {
                address: Address::from_base58(&self.get_new_address()?)?,
                content: output_content_for_asset(base_asset, amount_base),
                memo: Vec::new(),
            });
        }
        if amount_quote != 0 {
            outputs.push(Output {
                address: Address::from_base58(&self.get_new_address()?)?,
                content: output_content_for_asset(quote_asset, amount_quote),
                memo: Vec::new(),
            });
        }
        self.broadcast_constructor(
            vec![selected],
            outputs,
            TransactionData::DutchAuctionCollect {
                asset_offered: base_asset,
                asset_receive: quote_asset,
                amount_offered_remaining: amount_base,
                amount_received: amount_quote,
            },
        )
    }

    fn broadcast_constructor(
        &mut self,
        selected: Vec<WalletUtxo>,
        outputs: Vec<Output>,
        data: TransactionData,
    ) -> Result<Value, Error> {
        let transaction = Transaction {
            inputs: selected_to_outpoints(&selected)?,
            outputs,
            memo: Vec::new(),
            data: Some(data),
        };
        let txid = self.sign_and_broadcast(transaction, &selected)?;
        Ok(json!({
            "txid": txid,
            "status": "broadcast",
            "native": true,
        }))
    }

    fn sign_and_broadcast(
        &self,
        transaction: Transaction,
        selected: &[WalletUtxo],
    ) -> Result<Value, Error> {
        let tx_bytes = borsh::to_vec(&transaction)?;
        let mut authorizations = Vec::with_capacity(selected.len());
        for utxo in selected {
            let index = self.index_for_address(&utxo.address)?;
            let signing_key = self.signing_key(index)?;
            let verifying_key = VerifyingKey(signing_key.verifying_key());
            if verifying_key.address().as_base58() != utxo.address {
                return Err(Error::NoSigningAddress(utxo.address.clone()));
            }
            let message = [&[0u8], tx_bytes.as_slice()].concat();
            authorizations.push(Authorization {
                verifying_key,
                signature: Signature(signing_key.sign(&message)),
            });
        }

        let authorized = AuthorizedTransaction {
            transaction,
            authorizations,
        };
        let authorized_hex = hex::encode(borsh::to_vec(&authorized)?);
        bitassets_rpc_call_with_params(
            &self.rpc_url,
            "submit_authorized_transaction",
            vec![json!(authorized_hex)],
        )
    }

    fn select_wallet_utxos(
        &self,
        asset: &AssetId,
        amount: u64,
    ) -> Result<(u64, Vec<WalletUtxo>), Error> {
        let key = asset_key(asset);
        let mut selected = Vec::new();
        let mut total = 0u64;
        for utxo in self
            .stored
            .confirmed_utxos
            .iter()
            .filter(|utxo| utxo.asset_id == key)
        {
            selected.push(utxo.clone());
            total = total
                .checked_add(utxo.amount)
                .ok_or_else(|| Error::Rpc("amount overflow".to_string()))?;
            if total >= amount {
                break;
            }
        }
        if total < amount {
            Err(Error::NotEnoughFunds {
                asset_id: key,
                amount,
            })
        } else {
            Ok((total, selected))
        }
    }

    fn select_lp_utxos(
        &self,
        asset0: &str,
        asset1: &str,
        amount: u64,
    ) -> Result<(u64, Vec<WalletUtxo>), Error> {
        let mut selected = Vec::new();
        let mut total = 0u64;
        for utxo in self.stored.confirmed_utxos.iter().filter(|utxo| {
            utxo.content_kind == "amm_lp_token"
                && utxo.lp_asset0.as_deref() == Some(asset0)
                && utxo.lp_asset1.as_deref() == Some(asset1)
        }) {
            selected.push(utxo.clone());
            total = total
                .checked_add(utxo.amount)
                .ok_or_else(|| Error::Rpc("amount overflow".to_string()))?;
            if total >= amount {
                break;
            }
        }
        if total < amount {
            Err(Error::NotEnoughFunds {
                asset_id: format!("lp:{asset0}:{asset1}"),
                amount,
            })
        } else {
            Ok((total, selected))
        }
    }

    fn select_auction_receipt(&self, auction_id: &str) -> Result<WalletUtxo, Error> {
        self.stored
            .confirmed_utxos
            .iter()
            .find(|utxo| {
                utxo.content_kind == "dutch_auction_receipt"
                    && utxo.auction_id.as_deref() == Some(auction_id)
            })
            .cloned()
            .ok_or_else(|| Error::NoWalletUtxo(format!("auction receipt {auction_id}")))
    }

    fn push_change(
        &mut self,
        outputs: &mut Vec<Output>,
        asset: AssetId,
        amount: u64,
    ) -> Result<(), Error> {
        if amount != 0 {
            outputs.push(Output {
                address: Address::from_base58(&self.get_new_address()?)?,
                content: output_content_for_asset(asset, amount),
                memo: Vec::new(),
            });
        }
        Ok(())
    }

    fn apply_update(&mut self, update: &Value) -> Result<(), Error> {
        let proof_refs = parse_proof_refs(update.get("proof_refs"))?;
        let utreexo_view = parse_utreexo_view(update)?;
        let spent = parse_outpoints(update.get("spent_outpoints"))?
            .into_iter()
            .chain(parse_outpoints(update.get("mempool_spent_outpoints"))?)
            .collect::<BTreeSet<_>>();
        self.stored
            .confirmed_utxos
            .retain(|utxo| !spent.contains(&utxo.outpoint));
        self.stored.spent_outpoints = spent.iter().cloned().collect();

        for utxo in parse_utxos(
            update.get("created_utxos"),
            true,
            &proof_refs,
            Some(&utreexo_view),
        )? {
            upsert_utxo(&mut self.stored.confirmed_utxos, utxo);
        }
        self.stored.mempool_utxos =
            parse_utxos(update.get("mempool_created_utxos"), false, &[], None)?;
        self.stored.last_tip_hash = update
            .get("tip_hash")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        self.stored.last_tip_height = update
            .get("tip_height")
            .and_then(Value::as_u64)
            .and_then(|height| u32::try_from(height).ok());
        Ok(())
    }

    fn balances(&self) -> BTreeMap<String, u64> {
        let mut balances = BTreeMap::new();
        for utxo in &self.stored.confirmed_utxos {
            let entry = balances.entry(utxo.asset_id.clone()).or_insert(0u64);
            *entry = entry.saturating_add(utxo.amount);
        }
        balances
    }

    fn save(&self) -> Result<(), Error> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut stored = self.stored.clone();
        if !self.persist_seed {
            stored.seed_hex.clear();
        }
        let bytes = serde_json::to_vec_pretty(&stored)?;
        let temp_path = self.path.with_extension("tmp");
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        options.mode(0o600);
        {
            let mut file = options.open(&temp_path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        fs::rename(temp_path, &self.path)?;
        Ok(())
    }

    fn seed(&self) -> Result<[u8; 64], Error> {
        if let Some(seed_hex) = &self.runtime_seed_hex {
            return decode_seed(seed_hex);
        }
        decode_seed(&self.stored.seed_hex)
    }

    fn signing_key(&self, index: u32) -> Result<ed25519_dalek::SigningKey, Error> {
        let master = Xpriv::new_master(bitcoin::NetworkKind::Test, &self.seed()?)?;
        let derivation_path = DerivationPath::master()
            .child(ChildNumber::Hardened { index: 0 })
            .child(ChildNumber::Normal { index });
        let xpriv = master.derive_priv(&bitcoin::key::Secp256k1::new(), &derivation_path)?;
        Ok(ed25519_dalek::SigningKey::from_bytes(
            &xpriv.private_key.secret_bytes(),
        ))
    }

    fn index_for_address(&self, address: &str) -> Result<u32, Error> {
        self.stored
            .addresses
            .iter()
            .find(|entry| entry.address == address)
            .map(|entry| entry.index)
            .ok_or_else(|| Error::NoSigningAddress(address.to_string()))
    }
}

fn decode_seed(seed_hex: &str) -> Result<[u8; 64], Error> {
    let bytes = hex::decode(seed_hex)?;
    let len = bytes.len();
    bytes
        .try_into()
        .map_err(|_: Vec<u8>| Error::SeedLength(len))
}

fn parse_outpoints(value: Option<&Value>) -> Result<Vec<WalletOutPoint>, Error> {
    let Some(array) = value.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    array.iter().map(parse_outpoint).collect()
}

fn parse_outpoint(value: &Value) -> Result<WalletOutPoint, Error> {
    let regular = value
        .get("Regular")
        .ok_or_else(|| Error::Rpc("lite wallet V1 only supports regular outpoints".to_string()))?;
    let txid = regular
        .get("txid")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Rpc("regular outpoint missing txid".to_string()))?
        .to_string();
    let vout = regular
        .get("vout")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::Rpc("regular outpoint missing vout".to_string()))?;
    Ok(WalletOutPoint {
        txid,
        vout: u32::try_from(vout)
            .map_err(|_| Error::Rpc("regular outpoint vout overflow".to_string()))?,
    })
}

#[derive(Clone, Debug)]
struct UtreexoProofRef {
    outpoint: WalletOutPoint,
    leaf_hash: String,
    proof: Proof,
}

#[derive(Clone, Debug)]
struct UtreexoView {
    stump: Stump,
    proofs: Vec<UtreexoProofRef>,
}

impl UtreexoView {
    fn verify(&self, proof: &UtreexoProofRef) -> Result<(), Error> {
        let leaf = parse_node_hash(&proof.leaf_hash)?;
        let valid = self
            .stump
            .verify(&proof.proof, &[leaf])
            .map_err(|err| Error::Rpc(format!("Utreexo proof verification failed: {err:?}")))?;
        if valid {
            Ok(())
        } else {
            Err(Error::UtreexoProof(format_outpoint(&proof.outpoint)))
        }
    }
}

fn parse_utreexo_view(update: &Value) -> Result<UtreexoView, Error> {
    let leaves = update
        .get("utreexo_leaf_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::Rpc("lite wallet update missing utreexo_leaf_count".to_string()))?;
    let roots = update
        .get("utreexo_roots")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Rpc("lite wallet update missing utreexo_roots".to_string()))?
        .iter()
        .map(|root| {
            root.as_str()
                .ok_or_else(|| Error::Rpc("Utreexo root must be a string".to_string()))
                .and_then(parse_node_hash)
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let proofs = update
        .get("utreexo_proofs")
        .and_then(Value::as_array)
        .map(|proofs| {
            proofs
                .iter()
                .map(|proof| {
                    let outpoint = parse_outpoint(proof.get("outpoint").ok_or_else(|| {
                        Error::Rpc("Utreexo proof missing outpoint".to_string())
                    })?)?;
                    let leaf_hash = proof
                        .get("leaf_hash")
                        .and_then(Value::as_str)
                        .ok_or_else(|| Error::Rpc("Utreexo proof missing leaf_hash".to_string()))?
                        .to_string();
                    let targets = proof
                        .get("targets")
                        .and_then(Value::as_array)
                        .ok_or_else(|| Error::Rpc("Utreexo proof missing targets".to_string()))?
                        .iter()
                        .map(|target| {
                            target.as_u64().ok_or_else(|| {
                                Error::Rpc("Utreexo proof target must be u64".to_string())
                            })
                        })
                        .collect::<Result<Vec<_>, Error>>()?;
                    let hashes = proof
                        .get("hashes")
                        .and_then(Value::as_array)
                        .ok_or_else(|| Error::Rpc("Utreexo proof missing hashes".to_string()))?
                        .iter()
                        .map(|hash| {
                            hash.as_str()
                                .ok_or_else(|| {
                                    Error::Rpc("Utreexo proof hash must be a string".to_string())
                                })
                                .and_then(parse_node_hash)
                        })
                        .collect::<Result<Vec<_>, Error>>()?;
                    Ok(UtreexoProofRef {
                        outpoint,
                        leaf_hash,
                        proof: Proof { targets, hashes },
                    })
                })
                .collect::<Result<Vec<_>, Error>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(UtreexoView {
        stump: Stump { leaves, roots },
        proofs,
    })
}

fn parse_node_hash(hex_hash: &str) -> Result<BitcoinNodeHash, Error> {
    let bytes = hex::decode(hex_hash)?;
    let len = bytes.len();
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_: Vec<u8>| Error::Rpc(format!("expected 32-byte Utreexo hash, got {len}")))?;
    Ok(BitcoinNodeHash::from(bytes))
}

fn format_outpoint(outpoint: &WalletOutPoint) -> String {
    format!("regular {} {}", outpoint.txid, outpoint.vout)
}

fn lite_wallet_leaf_hash(
    outpoint: &WalletOutPoint,
    address: &str,
    content_descriptor: &str,
    memo_hex: &str,
    proof_ref: &WalletProofRef,
) -> Result<String, Error> {
    let address = Address::from_base58(address)?;
    let memo = hex::decode(memo_hex)?;
    let payload = borsh::to_vec(&(
        "plain-bitassets:lite-wallet-leaf:v1",
        format_outpoint(outpoint),
        address.0,
        content_descriptor.to_string(),
        memo,
        proof_ref.sidechain_block_height.unwrap_or_default(),
        proof_ref.block_hash.clone().unwrap_or_default(),
    ))?;
    Ok(hex::encode(blake3::hash(&payload).as_bytes()))
}

#[derive(Clone, Debug)]
struct ParsedContent {
    content_kind: String,
    asset_id: String,
    amount: u64,
    descriptor: String,
    reservation_txid: Option<String>,
    reservation_commitment: Option<String>,
    lp_asset0: Option<String>,
    lp_asset1: Option<String>,
    auction_id: Option<String>,
}

fn parse_filled_content(content: &Value) -> Result<ParsedContent, Error> {
    if let Some(bitasset) = content.get("BitAsset").and_then(Value::as_array) {
        let asset_id = bitasset
            .first()
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Rpc("BitAsset UTXO missing asset id".to_string()))?
            .to_string();
        let amount = bitasset
            .get(1)
            .and_then(Value::as_u64)
            .ok_or_else(|| Error::Rpc("BitAsset UTXO missing amount".to_string()))?;
        return Ok(ParsedContent {
            content_kind: "bitasset".to_string(),
            asset_id: asset_id.clone(),
            amount,
            descriptor: format!("bitasset:{asset_id}:{amount}"),
            reservation_txid: None,
            reservation_commitment: None,
            lp_asset0: None,
            lp_asset1: None,
            auction_id: None,
        });
    }
    if let Some(asset_id) = content.get("BitAssetControl").and_then(Value::as_str) {
        return Ok(ParsedContent {
            content_kind: "bitasset_control".to_string(),
            asset_id: format!("control:{asset_id}"),
            amount: 1,
            descriptor: format!("bitasset-control:{asset_id}"),
            reservation_txid: None,
            reservation_commitment: None,
            lp_asset0: None,
            lp_asset1: None,
            auction_id: None,
        });
    }
    if let Some(reservation) = content.get("BitAssetReservation").and_then(Value::as_array) {
        let reservation_txid = reservation
            .first()
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Rpc("reservation UTXO missing txid".to_string()))?
            .to_string();
        let commitment = reservation
            .get(1)
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Rpc("reservation UTXO missing commitment".to_string()))?
            .to_string();
        return Ok(ParsedContent {
            content_kind: "reservation".to_string(),
            asset_id: format!("reservation:{commitment}"),
            amount: 1,
            descriptor: format!("reservation:{reservation_txid}:{commitment}"),
            reservation_txid: Some(reservation_txid),
            reservation_commitment: Some(commitment),
            lp_asset0: None,
            lp_asset1: None,
            auction_id: None,
        });
    }
    if let Some(lp_token) = content.get("AmmLpToken").and_then(Value::as_object) {
        let asset0_value = lp_token
            .get("asset0")
            .ok_or_else(|| Error::Rpc("LP token missing asset0".to_string()))?;
        let asset1_value = lp_token
            .get("asset1")
            .ok_or_else(|| Error::Rpc("LP token missing asset1".to_string()))?;
        let asset0_wire = asset_id_wire_string(asset0_value)?;
        let asset1_wire = asset_id_wire_string(asset1_value)?;
        let amount = lp_token
            .get("amount")
            .and_then(Value::as_u64)
            .ok_or_else(|| Error::Rpc("LP token missing amount".to_string()))?;
        let asset0 = asset_key_from_wire_hex(&asset0_wire)?;
        let asset1 = asset_key_from_wire_hex(&asset1_wire)?;
        return Ok(ParsedContent {
            content_kind: "amm_lp_token".to_string(),
            asset_id: format!("lp:{asset0}:{asset1}"),
            amount,
            descriptor: format!("amm-lp:{asset0_wire}:{asset1_wire}:{amount}"),
            reservation_txid: None,
            reservation_commitment: None,
            lp_asset0: Some(asset0),
            lp_asset1: Some(asset1),
            auction_id: None,
        });
    }
    if let Some(value) = content
        .get("BitcoinSats")
        .or_else(|| content.get("Bitcoin"))
        .and_then(Value::as_u64)
    {
        return Ok(ParsedContent {
            content_kind: "bitcoin".to_string(),
            asset_id: "bitcoin".to_string(),
            amount: value,
            descriptor: format!("bitcoin:{value}"),
            reservation_txid: None,
            reservation_commitment: None,
            lp_asset0: None,
            lp_asset1: None,
            auction_id: None,
        });
    }
    if let Some(auction_id) = content.get("DutchAuctionReceipt").and_then(Value::as_str) {
        return Ok(ParsedContent {
            content_kind: "dutch_auction_receipt".to_string(),
            asset_id: format!("auction-receipt:{auction_id}"),
            amount: 1,
            descriptor: format!("dutch-auction:{auction_id}"),
            reservation_txid: None,
            reservation_commitment: None,
            lp_asset0: None,
            lp_asset1: None,
            auction_id: Some(auction_id.to_string()),
        });
    }
    Ok(ParsedContent {
        content_kind: "unsupported".to_string(),
        asset_id: "unsupported".to_string(),
        amount: 0,
        descriptor: format!("unsupported:{content}"),
        reservation_txid: None,
        reservation_commitment: None,
        lp_asset0: None,
        lp_asset1: None,
        auction_id: None,
    })
}

fn parse_utxos(
    value: Option<&Value>,
    confirmed: bool,
    proof_refs: &[WalletProofRef],
    utreexo_view: Option<&UtreexoView>,
) -> Result<Vec<WalletUtxo>, Error> {
    let Some(array) = value.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    array
        .iter()
        .filter_map(|value| parse_utxo(value, confirmed, proof_refs, utreexo_view).transpose())
        .collect()
}

fn parse_utxo(
    value: &Value,
    confirmed: bool,
    proof_refs: &[WalletProofRef],
    utreexo_view: Option<&UtreexoView>,
) -> Result<Option<WalletUtxo>, Error> {
    let outpoint = parse_outpoint(
        value
            .get("outpoint")
            .ok_or_else(|| Error::Rpc("UTXO missing outpoint".to_string()))?,
    )?;
    let output = value
        .get("output")
        .ok_or_else(|| Error::Rpc("UTXO missing output".to_string()))?;
    let address = output
        .get("address")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::Rpc("UTXO missing address".to_string()))?
        .to_string();
    let content = output
        .get("content")
        .ok_or_else(|| Error::Rpc("UTXO missing content".to_string()))?;
    let parsed_content = parse_filled_content(content)?;
    if parsed_content.content_kind == "unsupported" {
        return Ok(None);
    }
    let refs = proof_refs
        .iter()
        .filter(|proof| proof.txid == outpoint.txid)
        .cloned()
        .collect::<Vec<_>>();
    let utreexo_leaf_hash = if confirmed {
        validate_confirmed_proof_refs(&outpoint, &refs)?;
        let view = utreexo_view
            .ok_or_else(|| Error::Rpc("confirmed UTXO missing Utreexo view".to_string()))?;
        let proof = view
            .proofs
            .iter()
            .find(|proof| proof.outpoint == outpoint)
            .ok_or_else(|| Error::UtreexoProof(format_outpoint(&outpoint)))?;
        let proof_ref = refs
            .first()
            .ok_or_else(|| Error::UtreexoProof(format_outpoint(&outpoint)))?;
        let expected_leaf_hash = lite_wallet_leaf_hash(
            &outpoint,
            &address,
            &parsed_content.descriptor,
            output
                .get("memo")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            proof_ref,
        )?;
        if expected_leaf_hash != proof.leaf_hash {
            return Err(Error::UtreexoProof(format_outpoint(&outpoint)));
        }
        view.verify(proof)?;
        Some(proof.leaf_hash.clone())
    } else {
        None
    };
    Ok(Some(WalletUtxo {
        outpoint: outpoint.clone(),
        address,
        asset_id: parsed_content.asset_id,
        amount: parsed_content.amount,
        confirmed,
        proof_refs: refs,
        utreexo_leaf_hash,
        content_kind: parsed_content.content_kind,
        reservation_txid: parsed_content.reservation_txid,
        reservation_commitment: parsed_content.reservation_commitment,
        lp_asset0: parsed_content.lp_asset0,
        lp_asset1: parsed_content.lp_asset1,
        auction_id: parsed_content.auction_id,
    }))
}

fn validate_confirmed_proof_refs(
    outpoint: &WalletOutPoint,
    proof_refs: &[WalletProofRef],
) -> Result<(), Error> {
    if proof_refs.is_empty() {
        return Err(Error::UtreexoProof(format_outpoint(outpoint)));
    }
    for proof_ref in proof_refs {
        if proof_ref.sidechain_block_height.is_none()
            || proof_ref.bmm_inclusions.is_empty()
            || proof_ref.best_main_verification.is_none()
        {
            return Err(Error::UtreexoProof(format_outpoint(outpoint)));
        }
    }
    Ok(())
}

fn parse_proof_refs(value: Option<&Value>) -> Result<Vec<WalletProofRef>, Error> {
    let Some(array) = value.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    array
        .iter()
        .map(|proof| {
            let txid = proof
                .get("txid")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Rpc("proof ref missing txid".to_string()))?
                .to_string();
            let bmm_inclusions = proof
                .get("bmm_inclusions")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(ToOwned::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            Ok(WalletProofRef {
                txid,
                block_hash: proof
                    .get("block_hash")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                sidechain_block_height: proof
                    .get("sidechain_block_height")
                    .and_then(Value::as_u64)
                    .and_then(|height| u32::try_from(height).ok()),
                bmm_inclusions,
                best_main_verification: proof
                    .get("best_main_verification")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            })
        })
        .collect()
}

fn upsert_utxo(utxos: &mut Vec<WalletUtxo>, utxo: WalletUtxo) {
    if let Some(existing) = utxos
        .iter_mut()
        .find(|existing| existing.outpoint == utxo.outpoint)
    {
        *existing = utxo;
    } else {
        utxos.push(utxo);
    }
}

fn reject_nonzero_fee(fee_sats: u64) -> Result<(), Error> {
    if fee_sats == 0 {
        Ok(())
    } else {
        Err(Error::Rpc(
            "native BitAssets constructors currently support fee_sats=0".to_string(),
        ))
    }
}

fn selected_to_outpoints(selected: &[WalletUtxo]) -> Result<Vec<OutPoint>, Error> {
    selected
        .iter()
        .map(|utxo| {
            Ok(OutPoint::Regular {
                txid: Txid(Hash::from_hex(&utxo.outpoint.txid)?),
                vout: utxo.outpoint.vout,
            })
        })
        .collect()
}

pub fn parse_asset_id(asset: &str) -> Result<AssetId, Error> {
    let normalized = asset.trim();
    if normalized.eq_ignore_ascii_case("bitcoin") {
        return Ok(AssetId::Bitcoin);
    }
    if let Some(bitasset) = normalized.strip_prefix("bitasset:") {
        return Ok(AssetId::BitAsset(BitAssetId(Hash::from_hex(bitasset)?)));
    }
    if let Some(control) = normalized.strip_prefix("control:") {
        return Ok(AssetId::BitAssetControl(BitAssetId(Hash::from_hex(
            control,
        )?)));
    }
    if let Ok(bytes) = hex::decode(normalized) {
        if let Ok(asset_id) = AssetId::try_from_wire_bytes(&bytes) {
            return Ok(asset_id);
        }
    }
    Ok(AssetId::BitAsset(BitAssetId(Hash::from_hex(normalized)?)))
}

impl AssetId {
    fn try_from_wire_bytes(bytes: &[u8]) -> Result<Self, Error> {
        borsh::from_slice(bytes)
            .map_err(|err| Error::Rpc(format!("invalid serialized asset id: {err}")))
    }
}

fn asset_id_wire_string(value: &Value) -> Result<String, Error> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| Error::Rpc("asset id must be a serialized hex string".to_string()))
}

fn asset_key_from_wire_hex(wire_hex: &str) -> Result<String, Error> {
    let bytes = hex::decode(wire_hex)?;
    let asset = AssetId::try_from_wire_bytes(&bytes)?;
    Ok(asset_key(&asset))
}

fn asset_key(asset: &AssetId) -> String {
    match asset {
        AssetId::Bitcoin => "bitcoin".to_string(),
        AssetId::BitAsset(bitasset) => hex::encode(bitasset.0 .0),
        AssetId::BitAssetControl(bitasset) => format!("control:{}", hex::encode(bitasset.0 .0)),
    }
}

fn output_content_for_asset(asset: AssetId, amount: u64) -> OutputContent {
    match asset {
        AssetId::Bitcoin => OutputContent::Bitcoin(amount),
        AssetId::BitAsset(_) => OutputContent::BitAsset(amount),
        AssetId::BitAssetControl(_) => OutputContent::BitAssetControl,
    }
}

fn unix_ms_now() -> Option<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn bitassets_rpc_call_with_params(
    rpc_url: &str,
    method: &str,
    params: Vec<Value>,
) -> Result<Value, Error> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": "floresta-native-bitassets-wallet",
        "method": method,
        "params": params
    });
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(BITASSETS_RPC_TIMEOUT))
            .build(),
    );
    let mut response = agent
        .post(rpc_url)
        .header("content-type", "application/json")
        .send_json(body)
        .map_err(|err| Error::Rpc(format!("request failed for {method}: {err}")))?;
    let value = response
        .body_mut()
        .read_json::<Value>()
        .map_err(|err| Error::Rpc(format!("invalid JSON response for {method}: {err}")))?;
    if let Some(error) = value.get("error").filter(|error| !error.is_null()) {
        return Err(Error::Rpc(format!("RPC error for {method}: {error}")));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| Error::Rpc(format!("RPC response for {method} did not include result")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO_SEED: &str = "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn derives_plain_bitassets_style_addresses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallet.json");
        let mut wallet = NativeBitAssetsWallet::open(
            &path,
            "http://127.0.0.1:6004".to_string(),
            Some(ZERO_SEED),
            true,
        )
        .unwrap();

        assert_eq!(
            wallet.get_new_address().unwrap(),
            "46wMdKN8vRVCHCKw77eqsRkpc6yT"
        );
        assert_eq!(
            wallet.get_new_address().unwrap(),
            "3GfJ72KNjMfLBpQTw2pBxq9XPSg1"
        );
    }

    #[test]
    fn can_keep_seed_out_of_persisted_wallet_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallet.json");
        let mut wallet = NativeBitAssetsWallet::open_with_seed_storage(
            &path,
            "http://127.0.0.1:6004".to_string(),
            Some(ZERO_SEED),
            true,
            false,
        )
        .unwrap();

        assert_eq!(
            wallet.get_new_address().unwrap(),
            "46wMdKN8vRVCHCKw77eqsRkpc6yT"
        );
        let persisted = fs::read_to_string(&path).unwrap();
        assert!(persisted.contains("\"seed_hex\": \"\""));
        assert!(!persisted.contains(ZERO_SEED));

        let mut reloaded = NativeBitAssetsWallet::open_with_seed_storage(
            &path,
            "http://127.0.0.1:6004".to_string(),
            Some(ZERO_SEED),
            false,
            false,
        )
        .unwrap();
        assert_eq!(
            reloaded.get_new_address().unwrap(),
            "3GfJ72KNjMfLBpQTw2pBxq9XPSg1"
        );
    }

    #[cfg(unix)]
    #[test]
    fn persisted_wallet_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallet.json");
        let mut wallet = NativeBitAssetsWallet::open(
            &path,
            "http://127.0.0.1:6004".to_string(),
            Some(ZERO_SEED),
            true,
        )
        .unwrap();
        wallet.get_new_address().unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn native_wallet_smoke_persists_proof_backed_utxos() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallet.json");
        let mut wallet = NativeBitAssetsWallet::open(
            &path,
            "http://127.0.0.1:6004".to_string(),
            Some(ZERO_SEED),
            true,
        )
        .unwrap();

        let outpoint = WalletOutPoint {
            txid: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            vout: 0,
        };
        let proof_ref = WalletProofRef {
            txid: outpoint.txid.clone(),
            block_hash: Some(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            ),
            sidechain_block_height: Some(7),
            bmm_inclusions: vec!["bmm-inclusion-1".to_string()],
            best_main_verification: Some("verified".to_string()),
        };
        let leaf_hash = lite_wallet_leaf_hash(
            &outpoint,
            "XdVwC9EcS3AYYXVgLFswjwxXGrJ",
            "bitasset:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc:42",
            "",
            &proof_ref,
        )
        .unwrap();
        let update = json!({
            "tip_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "tip_height": 7,
            "utreexo_leaf_count": 1,
            "utreexo_roots": [leaf_hash],
            "created_utxos": [{
                "outpoint": {"Regular": {
                    "txid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "vout": 0
                }},
                "output": {
                    "address": "XdVwC9EcS3AYYXVgLFswjwxXGrJ",
                    "content": {"BitAsset": [
                        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                        42
                    ]},
                    "memo": ""
                }
            }],
            "spent_outpoints": [],
            "mempool_created_utxos": [],
            "mempool_spent_outpoints": [],
            "proof_refs": [{
                "txid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "block_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "sidechain_block_height": 7,
                "bmm_inclusions": ["bmm-inclusion-1"],
                "best_main_verification": "verified"
            }],
            "utreexo_proofs": [{
                "outpoint": {"Regular": {
                    "txid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "vout": 0
                }},
                "leaf_hash": leaf_hash,
                "targets": [0],
                "hashes": []
            }]
        });

        wallet.apply_update(&update).unwrap();
        wallet.save().unwrap();
        let reloaded =
            NativeBitAssetsWallet::open(&path, "http://127.0.0.1:6004".to_string(), None, false)
                .unwrap();

        assert_eq!(reloaded.stored.confirmed_utxos.len(), 1);
        assert_eq!(reloaded.balances().values().copied().sum::<u64>(), 42);
        assert_eq!(reloaded.stored.last_tip_height, Some(7));

        let persisted_utxo = &reloaded.stored.confirmed_utxos[0];
        assert_eq!(
            persisted_utxo.utreexo_leaf_hash.as_deref(),
            Some(leaf_hash.as_str())
        );
        assert_eq!(persisted_utxo.proof_refs.len(), 1);
        assert_eq!(persisted_utxo.proof_refs[0].sidechain_block_height, Some(7));
        assert_eq!(
            persisted_utxo.proof_refs[0].bmm_inclusions,
            vec!["bmm-inclusion-1".to_string()]
        );
        assert_eq!(
            persisted_utxo.proof_refs[0]
                .best_main_verification
                .as_deref(),
            Some("verified")
        );

        let listed = reloaded.list_utxos();
        let confirmed = listed["confirmed"].as_array().unwrap();
        assert_eq!(confirmed[0]["utreexo_leaf_hash"], leaf_hash);
        assert_eq!(confirmed[0]["proof_refs"][0]["sidechain_block_height"], 7);
        assert_eq!(
            confirmed[0]["proof_refs"][0]["bmm_inclusions"][0],
            "bmm-inclusion-1"
        );
        assert_eq!(
            confirmed[0]["proof_refs"][0]["best_main_verification"],
            "verified"
        );
    }

    #[test]
    fn borsh_transfer_shape_is_stable() {
        let tx = Transaction {
            inputs: vec![OutPoint::Regular {
                txid: Txid(Hash([1; 32])),
                vout: 2,
            }],
            outputs: vec![Output {
                address: Address([3; 20]),
                content: OutputContent::BitAsset(42),
                memo: Vec::new(),
            }],
            memo: Vec::new(),
            data: None,
        };

        assert_eq!(
            hex::encode(borsh::to_vec(&tx).unwrap()),
            "0100000000010101010101010101010101010101010101010101010101010101010101010102000000010000000303030303030303030303030303030303030303022a00000000000000000000000000000000"
        );
    }

    fn constructor_shape_hashes() -> Vec<(&'static str, String)> {
        let asset_a = AssetId::BitAsset(BitAssetId(Hash([0x11; 32])));
        let asset_b = AssetId::BitAsset(BitAssetId(Hash([0x22; 32])));
        let auction_id = DutchAuctionId(Txid(Hash([0x44; 32])));
        let outpoint = |byte, vout| OutPoint::Regular {
            txid: Txid(Hash([byte; 32])),
            vout,
        };
        let output = |byte, content| Output {
            address: Address([byte; 20]),
            content,
            memo: Vec::new(),
        };
        let data = BitAssetData {
            commitment: None,
            socket_addr_v4: None,
            socket_addr_v6: None,
            encryption_pubkey: None,
            signing_pubkey: None,
        };
        let params = DutchAuctionParams {
            start_block: 10,
            duration: 5,
            base_asset: asset_a,
            base_amount: 100,
            quote_asset: asset_b,
            initial_price: 1_000,
            final_price: 500,
        };
        let txs = vec![
            (
                "reserve",
                Transaction {
                    inputs: Vec::new(),
                    outputs: vec![output(1, OutputContent::BitAssetReservation)],
                    memo: Vec::new(),
                    data: Some(TransactionData::BitAssetReservation {
                        commitment: Hash([0xaa; 32]),
                    }),
                },
            ),
            (
                "register",
                Transaction {
                    inputs: vec![outpoint(1, 0)],
                    outputs: vec![
                        output(2, OutputContent::BitAsset(1_000)),
                        output(2, OutputContent::BitAssetControl),
                    ],
                    memo: Vec::new(),
                    data: Some(TransactionData::BitAssetRegistration {
                        name_hash: Hash([0xbb; 32]),
                        revealed_nonce: Hash([0xcc; 32]),
                        bitasset_data: Box::new(data),
                        initial_supply: 1_000,
                    }),
                },
            ),
            (
                "amm_mint",
                Transaction {
                    inputs: vec![outpoint(0x11, 0), outpoint(0x22, 0)],
                    outputs: vec![output(3, OutputContent::AmmLpToken(200))],
                    memo: Vec::new(),
                    data: Some(TransactionData::AmmMint {
                        amount0: 100,
                        amount1: 400,
                        lp_token_mint: 200,
                    }),
                },
            ),
            (
                "amm_swap",
                Transaction {
                    inputs: vec![outpoint(0x11, 1)],
                    outputs: vec![output(4, OutputContent::BitAsset(36))],
                    memo: Vec::new(),
                    data: Some(TransactionData::AmmSwap {
                        amount_spent: 10,
                        amount_receive: 36,
                        pair_asset: asset_b,
                    }),
                },
            ),
            (
                "amm_burn",
                Transaction {
                    inputs: vec![outpoint(0x33, 0)],
                    outputs: vec![
                        output(5, OutputContent::BitAsset(10)),
                        output(6, OutputContent::BitAsset(40)),
                    ],
                    memo: Vec::new(),
                    data: Some(TransactionData::AmmBurn {
                        amount0: 10,
                        amount1: 40,
                        lp_token_burn: 20,
                    }),
                },
            ),
            (
                "dutch_create",
                Transaction {
                    inputs: vec![outpoint(0x11, 2)],
                    outputs: vec![output(7, OutputContent::DutchAuctionReceipt)],
                    memo: Vec::new(),
                    data: Some(TransactionData::DutchAuctionCreate(params.clone())),
                },
            ),
            (
                "dutch_bid",
                Transaction {
                    inputs: vec![outpoint(0x22, 1)],
                    outputs: vec![output(8, OutputContent::BitAsset(10))],
                    memo: Vec::new(),
                    data: Some(TransactionData::DutchAuctionBid {
                        auction_id,
                        receive_asset: asset_a,
                        quantity: 10,
                        bid_size: 100,
                    }),
                },
            ),
            (
                "dutch_collect",
                Transaction {
                    inputs: vec![outpoint(0x44, 0)],
                    outputs: vec![
                        output(9, OutputContent::BitAsset(90)),
                        output(10, OutputContent::BitAsset(100)),
                    ],
                    memo: Vec::new(),
                    data: Some(TransactionData::DutchAuctionCollect {
                        asset_offered: asset_a,
                        asset_receive: asset_b,
                        amount_offered_remaining: 90,
                        amount_received: 100,
                    }),
                },
            ),
        ];

        txs.into_iter()
            .map(|(label, tx)| {
                let bytes = borsh::to_vec(&tx).unwrap();
                (label, hex::encode(blake3::hash(&bytes).as_bytes()))
            })
            .collect()
    }

    #[test]
    fn borsh_native_constructor_shapes_are_stable() {
        assert_eq!(
            constructor_shape_hashes(),
            vec![
                (
                    "reserve",
                    "d6f27e504be264fbe6da94ef5eb8cfc39c86d96b12dd4ed1eec4c2b86c76a36e".to_string()
                ),
                (
                    "register",
                    "74a05698e0d29b5bab35e9706944231bc06a77751c71c8096bd010f56f27f5d1".to_string()
                ),
                (
                    "amm_mint",
                    "8b2a54a5e6976aa7d55682ec488770c7c2e77665db2f5a2ab6bcef38f1f77bb5".to_string()
                ),
                (
                    "amm_swap",
                    "d3edc01e9380fc79d1cc1221b531361fee29d508f5f8e1d173690c8178bbd3e2".to_string()
                ),
                (
                    "amm_burn",
                    "2b82ba8d1a4115dab1be1f4f3f9f8aa595760c84838021a880b7a3125a4524d4".to_string()
                ),
                (
                    "dutch_create",
                    "436afe9e94c3e61c86454d310493f13122ef61ea9934d96f0348d70e39cc24c2".to_string()
                ),
                (
                    "dutch_bid",
                    "18b16c34e76a7a03944ddeb42442c5a667714c5780a809633300cc0c2e1f8c0e".to_string()
                ),
                (
                    "dutch_collect",
                    "90f8a8687c1e868f59b7f822bcf138eda836cb5c7bc729aeba5e6d11dd1c9b23".to_string()
                ),
            ]
        );
    }

    fn native_test_wallet() -> NativeBitAssetsWallet {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallet.json");
        NativeBitAssetsWallet::open(
            &path,
            "http://127.0.0.1:6004".to_string(),
            Some(ZERO_SEED),
            true,
        )
        .unwrap()
    }

    fn assert_zero_fee_rejected(result: Result<Value, Error>) {
        match result {
            Err(Error::Rpc(message)) => assert!(message.contains("fee_sats=0")),
            other => panic!("expected fee_sats=0 rejection, got {other:?}"),
        }
    }

    #[test]
    fn native_constructors_reject_nonzero_fees_before_network_or_selection() {
        let asset_a = "1111111111111111111111111111111111111111111111111111111111111111";
        let asset_b = "2222222222222222222222222222222222222222222222222222222222222222";
        let auction_id = "4444444444444444444444444444444444444444444444444444444444444444";

        let mut wallet = native_test_wallet();
        assert_zero_fee_rejected(wallet.transfer(
            "46wMdKN8vRVCHCKw77eqsRkpc6yT",
            asset_a,
            1,
            1,
            None,
        ));
        assert_zero_fee_rejected(wallet.reserve("name", 1));
        assert_zero_fee_rejected(wallet.register(
            "name",
            1,
            BitAssetData {
                commitment: None,
                socket_addr_v4: None,
                socket_addr_v6: None,
                encryption_pubkey: None,
                signing_pubkey: None,
            },
            1,
        ));
        assert_zero_fee_rejected(wallet.amm_mint(asset_a, asset_b, 1, 1, 1, 1));
        assert_zero_fee_rejected(wallet.amm_swap(asset_a, asset_b, 1, 1, 1));
        assert_zero_fee_rejected(wallet.amm_burn(asset_a, asset_b, 1, 1, 1, 1));
        assert_zero_fee_rejected(wallet.dutch_auction_create(
            DutchAuctionParams {
                start_block: 1,
                duration: 1,
                base_asset: parse_asset_id(asset_a).unwrap(),
                base_amount: 1,
                quote_asset: parse_asset_id(asset_b).unwrap(),
                initial_price: 1,
                final_price: 1,
            },
            1,
        ));
        assert_zero_fee_rejected(wallet.dutch_auction_bid(auction_id, asset_a, asset_b, 1, 1, 1));
        assert_zero_fee_rejected(
            wallet.dutch_auction_collect(auction_id, asset_a, asset_b, 1, 1, 1),
        );
    }

    #[test]
    fn invalid_utreexo_proof_rejects_non_bitasset_wallet_content() {
        let mut wallet = native_test_wallet();
        let outpoint = WalletOutPoint {
            txid: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            vout: 0,
        };
        let proof_ref = WalletProofRef {
            txid: outpoint.txid.clone(),
            block_hash: Some(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            ),
            sidechain_block_height: Some(7),
            bmm_inclusions: vec!["bmm-inclusion-1".to_string()],
            best_main_verification: Some("verified".to_string()),
        };
        let commitment = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let leaf_hash = lite_wallet_leaf_hash(
            &outpoint,
            "XdVwC9EcS3AYYXVgLFswjwxXGrJ",
            &format!("reservation:{}:{commitment}", outpoint.txid),
            "",
            &proof_ref,
        )
        .unwrap();
        let update = json!({
            "tip_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "tip_height": 7,
            "utreexo_leaf_count": 1,
            "utreexo_roots": [leaf_hash],
            "created_utxos": [{
                "outpoint": {"Regular": {
                    "txid": outpoint.txid,
                    "vout": outpoint.vout
                }},
                "output": {
                    "address": "XdVwC9EcS3AYYXVgLFswjwxXGrJ",
                    "content": {"BitAssetReservation": [
                        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                        commitment
                    ]},
                    "memo": ""
                }
            }],
            "spent_outpoints": [],
            "mempool_created_utxos": [],
            "mempool_spent_outpoints": [],
            "proof_refs": [{
                "txid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "block_hash": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "sidechain_block_height": 7,
                "bmm_inclusions": ["bmm-inclusion-1"],
                "best_main_verification": "verified"
            }],
            "utreexo_proofs": [{
                "outpoint": {"Regular": {
                    "txid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    "vout": 0
                }},
                "leaf_hash": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                "targets": [0],
                "hashes": []
            }]
        });

        match wallet.apply_update(&update) {
            Err(Error::UtreexoProof(outpoint)) => {
                assert!(outpoint
                    .contains("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
            }
            other => panic!("expected Utreexo proof rejection, got {other:?}"),
        }
    }
}
