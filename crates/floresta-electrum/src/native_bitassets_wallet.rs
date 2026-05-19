use std::fs;
use std::path::{Path, PathBuf};

use bitcoin::base58;
use bitcoin::bip32::{ChildNumber, DerivationPath, Xpriv};
use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::Signer as _;
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, thiserror::Error)]
pub enum NativeBitAssetsWalletError {
    #[error("BitAssets native wallet already exists at {0}")]
    AlreadyExists(String),
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
    #[error("BitAssets native wallet has no address for input {0}")]
    NoSigningAddress(String),
    #[error("JSON error")]
    Json(#[from] serde_json::Error),
    #[error("BitAssets RPC error: {0}")]
    Rpc(String),
    #[error("native wallet seed must be exactly 64 bytes, got {0}")]
    SeedLength(usize),
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct Address([u8; 20]);

impl Address {
    fn from_base58(s: &str) -> Result<Self, NativeBitAssetsWalletError> {
        let decoded = base58::decode(s)?;
        let len = decoded.len();
        let bytes = decoded
            .try_into()
            .map_err(|_: Vec<u8>| NativeBitAssetsWalletError::AddressLength(len))?;
        Ok(Self(bytes))
    }

    fn as_base58(&self) -> String {
        base58::encode(&self.0)
    }
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct Hash([u8; 32]);

impl Hash {
    fn from_hex(s: &str) -> Result<Self, NativeBitAssetsWalletError> {
        Ok(Self(hex::decode(s)?.try_into().map_err(
            |bytes: Vec<u8>| {
                NativeBitAssetsWalletError::Rpc(format!(
                    "expected 32-byte hash, got {} bytes",
                    bytes.len()
                ))
            },
        )?))
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
enum AssetId {
    Bitcoin,
    BitAsset(BitAssetId),
    BitAssetControl(BitAssetId),
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct BitAssetId(Hash);

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
struct DutchAuctionId(Txid);

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Eq, PartialEq)]
struct DutchAuctionParams {
    start_block: u32,
    duration: u32,
    base_asset: AssetId,
    base_amount: u64,
    quote_asset: AssetId,
    initial_price: u64,
    final_price: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug, Eq, PartialEq)]
struct BitAssetData {
    ticker: Option<String>,
    name: Option<String>,
    summary: Option<String>,
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
struct VerifyingKey(ed25519_dalek::VerifyingKey);

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
        self.0.to_bytes().serialize(writer)
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

#[derive(Debug)]
struct NativeUtxo {
    outpoint: OutPoint,
    address: Address,
    amount: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredAddress {
    index: u32,
    address: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredWallet {
    version: u32,
    seed_hex: String,
    next_index: u32,
    addresses: Vec<StoredAddress>,
}

#[derive(Clone, Debug)]
pub struct NativeBitAssetsWallet {
    path: PathBuf,
    stored: StoredWallet,
}

impl NativeBitAssetsWallet {
    pub fn open(
        path: impl AsRef<Path>,
        seed_hex: Option<&str>,
        create: bool,
    ) -> Result<Self, NativeBitAssetsWalletError> {
        let path = path.as_ref().to_path_buf();
        if path.exists() {
            if create && seed_hex.is_some() {
                return Err(NativeBitAssetsWalletError::AlreadyExists(
                    path.display().to_string(),
                ));
            }
            let bytes = fs::read(&path)?;
            let stored = serde_json::from_slice(&bytes)?;
            return Ok(Self { path, stored });
        }

        if !create && seed_hex.is_none() {
            return Err(NativeBitAssetsWalletError::CreateRequired);
        }

        let seed = match seed_hex {
            Some(seed_hex) => decode_seed(seed_hex)?,
            None => {
                let mut seed = [0u8; 64];
                rand::rngs::OsRng.fill_bytes(&mut seed);
                seed
            }
        };
        let stored = StoredWallet {
            version: 1,
            seed_hex: hex::encode(seed),
            next_index: 0,
            addresses: Vec::new(),
        };
        let wallet = Self { path, stored };
        wallet.save()?;
        Ok(wallet)
    }

    pub fn get_new_address(&mut self) -> Result<String, NativeBitAssetsWalletError> {
        let index = self.stored.next_index;
        let signing_key = self.signing_key(index)?;
        let address = VerifyingKey(signing_key.verifying_key()).address();
        let address_string = address.as_base58();
        self.stored.next_index =
            self.stored.next_index.checked_add(1).ok_or_else(|| {
                NativeBitAssetsWalletError::Rpc("address index overflow".to_string())
            })?;
        self.stored.addresses.push(StoredAddress {
            index,
            address: address_string.clone(),
        });
        self.save()?;
        Ok(address_string)
    }

    pub fn transfer(
        &mut self,
        rpc_url: &str,
        destination: &str,
        asset_id_hex: &str,
        amount: u64,
        fee_sats: u64,
        memo: Option<Vec<u8>>,
    ) -> Result<Value, NativeBitAssetsWalletError> {
        if fee_sats != 0 {
            return Err(NativeBitAssetsWalletError::Rpc(
                "native BitAsset transfer currently supports fee_sats=0".to_string(),
            ));
        }
        let _asset_id = BitAssetId(Hash::from_hex(asset_id_hex)?);
        let mut selected = Vec::new();
        let mut total = 0u64;
        for utxo in self.native_bitasset_utxos(rpc_url, asset_id_hex)? {
            selected.push(utxo);
            total = selected
                .iter()
                .try_fold(0u64, |sum, utxo| sum.checked_add(utxo.amount))
                .ok_or_else(|| NativeBitAssetsWalletError::Rpc("amount overflow".to_string()))?;
            if total >= amount {
                break;
            }
        }
        if total < amount {
            return Err(NativeBitAssetsWalletError::NotEnoughFunds {
                asset_id: asset_id_hex.to_string(),
                amount,
            });
        }

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
            inputs: selected.iter().map(|utxo| utxo.outpoint).collect(),
            outputs,
            memo: Vec::new(),
            data: None,
        };
        let tx_bytes = borsh::to_vec(&transaction)?;
        let mut authorizations = Vec::with_capacity(selected.len());
        for utxo in &selected {
            let index = self.index_for_address(&utxo.address.as_base58())?;
            let signing_key = self.signing_key(index)?;
            let verifying_key = VerifyingKey(signing_key.verifying_key());
            if verifying_key.address() != utxo.address {
                return Err(NativeBitAssetsWalletError::NoSigningAddress(
                    utxo.address.as_base58(),
                ));
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
        let txid = bitassets_rpc_call_with_params(
            rpc_url,
            "submit_authorized_transaction",
            vec![json!(authorized_hex)],
        )?;
        Ok(json!({
            "txid": txid,
            "status": "broadcast",
            "native": true
        }))
    }

    fn native_bitasset_utxos(
        &self,
        rpc_url: &str,
        asset_id_hex: &str,
    ) -> Result<Vec<NativeUtxo>, NativeBitAssetsWalletError> {
        let utxos = bitassets_rpc_call_with_params(rpc_url, "list_utxos", vec![])?;
        let utxos = utxos.as_array().ok_or_else(|| {
            NativeBitAssetsWalletError::Rpc("list_utxos did not return an array".to_string())
        })?;
        let mut native = Vec::new();
        for utxo in utxos {
            let Some(address) = utxo.pointer("/output/address").and_then(Value::as_str) else {
                continue;
            };
            if self.index_for_address(address).is_err() {
                continue;
            }
            let Some(bitasset) = utxo
                .pointer("/output/content/BitAsset")
                .and_then(Value::as_array)
            else {
                continue;
            };
            let Some(utxo_asset_id) = bitasset.first().and_then(Value::as_str) else {
                continue;
            };
            if utxo_asset_id != asset_id_hex {
                continue;
            }
            let amount = bitasset.get(1).and_then(Value::as_u64).ok_or_else(|| {
                NativeBitAssetsWalletError::Rpc("BitAsset UTXO missing amount".to_string())
            })?;
            let regular = utxo.pointer("/outpoint/Regular").ok_or_else(|| {
                NativeBitAssetsWalletError::Rpc(
                    "native transfer only supports regular outpoints".to_string(),
                )
            })?;
            let txid = regular.get("txid").and_then(Value::as_str).ok_or_else(|| {
                NativeBitAssetsWalletError::Rpc("regular outpoint missing txid".to_string())
            })?;
            let vout = regular.get("vout").and_then(Value::as_u64).ok_or_else(|| {
                NativeBitAssetsWalletError::Rpc("regular outpoint missing vout".to_string())
            })?;
            let vout = u32::try_from(vout).map_err(|_| {
                NativeBitAssetsWalletError::Rpc("regular outpoint vout overflow".to_string())
            })?;
            native.push(NativeUtxo {
                outpoint: OutPoint::Regular {
                    txid: Txid(Hash::from_hex(txid)?),
                    vout,
                },
                address: Address::from_base58(address)?,
                amount,
            });
        }
        Ok(native)
    }

    fn save(&self) -> Result<(), NativeBitAssetsWalletError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&self.stored)?;
        fs::write(&self.path, bytes)?;
        Ok(())
    }

    fn seed(&self) -> Result<[u8; 64], NativeBitAssetsWalletError> {
        decode_seed(&self.stored.seed_hex)
    }

    fn signing_key(
        &self,
        index: u32,
    ) -> Result<ed25519_dalek::SigningKey, NativeBitAssetsWalletError> {
        let master = Xpriv::new_master(bitcoin::NetworkKind::Test, &self.seed()?)?;
        let derivation_path = DerivationPath::master()
            .child(ChildNumber::Hardened { index: 0 })
            .child(ChildNumber::Normal { index });
        let xpriv = master.derive_priv(&bitcoin::key::Secp256k1::new(), &derivation_path)?;
        Ok(ed25519_dalek::SigningKey::from_bytes(
            &xpriv.private_key.secret_bytes(),
        ))
    }

    fn index_for_address(&self, address: &str) -> Result<u32, NativeBitAssetsWalletError> {
        self.stored
            .addresses
            .iter()
            .find(|entry| entry.address == address)
            .map(|entry| entry.index)
            .ok_or_else(|| NativeBitAssetsWalletError::NoSigningAddress(address.to_string()))
    }
}

fn decode_seed(seed_hex: &str) -> Result<[u8; 64], NativeBitAssetsWalletError> {
    let bytes = hex::decode(seed_hex)?;
    let len = bytes.len();
    bytes
        .try_into()
        .map_err(|_: Vec<u8>| NativeBitAssetsWalletError::SeedLength(len))
}

fn bitassets_rpc_call_with_params(
    rpc_url: &str,
    method: &str,
    params: Vec<Value>,
) -> Result<Value, NativeBitAssetsWalletError> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": "floresta-native-bitassets-wallet",
        "method": method,
        "params": params
    });
    let mut response = ureq::post(rpc_url)
        .header("content-type", "application/json")
        .send_json(body)
        .map_err(|err| {
            NativeBitAssetsWalletError::Rpc(format!("request failed for {method}: {err}"))
        })?;
    let value = response.body_mut().read_json::<Value>().map_err(|err| {
        NativeBitAssetsWalletError::Rpc(format!("invalid JSON response for {method}: {err}"))
    })?;
    if let Some(error) = value.get("error").filter(|error| !error.is_null()) {
        return Err(NativeBitAssetsWalletError::Rpc(format!(
            "RPC error for {method}: {error}"
        )));
    }
    value.get("result").cloned().ok_or_else(|| {
        NativeBitAssetsWalletError::Rpc(format!("RPC response for {method} did not include result"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZERO_SEED: &str = "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn derives_plain_bitassets_style_addresses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wallet.json");
        let mut wallet = NativeBitAssetsWallet::open(&path, Some(ZERO_SEED), true).unwrap();

        assert_eq!(
            wallet.get_new_address().unwrap(),
            "XdVwC9EcS3AYYXVgLFswjwxXGrJ"
        );
        assert_eq!(
            wallet.get_new_address().unwrap(),
            "XpaAKxPGh3fCjCwd8SjHG3iAe9e"
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
        let bytes = borsh::to_vec(&tx).unwrap();
        assert_eq!(
            hex::encode(bytes),
            "010000000001010101010101010101010101010101010101010101010101010101010101010102000000010000000303030303030303030303030303030303030303022a000000000000000000000000"
        );
    }
}
