// Based on slip132 from LNP/BP Descriptor Wallet library by:
//     Dr. Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Adapted for Floresta by:
//     Davidson Sousa <me@dlsouza.lol>

//! Bitcoin SLIP-132 standard implementation for parsing custom xpub/xpriv key
//! formats

use std::fmt::Debug;

use bitcoin::base58;
use bitcoin::bip32;
use bitcoin::bip32::Xpriv;
use bitcoin::bip32::Xpub;

/// Magical version bytes for xpub: bitcoin mainnet public key for P2PKH or P2SH
pub const VERSION_MAGIC_XPUB: [u8; 4] = [0x04, 0x88, 0xB2, 0x1E];
/// Magical version bytes for xprv: bitcoin mainnet private key for P2PKH or
/// P2SH
pub const VERSION_MAGIC_XPRV: [u8; 4] = [0x04, 0x88, 0xAD, 0xE4];
/// Magical version bytes for ypub: bitcoin mainnet public key for P2WPKH in
/// P2SH
pub const VERSION_MAGIC_YPUB: [u8; 4] = [0x04, 0x9D, 0x7C, 0xB2];
/// Magical version bytes for yprv: bitcoin mainnet private key for P2WPKH in
/// P2SH
pub const VERSION_MAGIC_YPRV: [u8; 4] = [0x04, 0x9D, 0x78, 0x78];
/// Magical version bytes for zpub: bitcoin mainnet public key for P2WPKH
pub const VERSION_MAGIC_ZPUB: [u8; 4] = [0x04, 0xB2, 0x47, 0x46];
/// Magical version bytes for zprv: bitcoin mainnet private key for P2WPKH
pub const VERSION_MAGIC_ZPRV: [u8; 4] = [0x04, 0xB2, 0x43, 0x0C];
/// Magical version bytes for Ypub: bitcoin mainnet public key for
/// multi-signature P2WSH in P2SH
pub const VERSION_MAGIC_YPUB_MULTISIG: [u8; 4] = [0x02, 0x95, 0xb4, 0x3f];
/// Magical version bytes for Yprv: bitcoin mainnet private key for
/// multi-signature P2WSH in P2SH
pub const VERSION_MAGIC_YPRV_MULTISIG: [u8; 4] = [0x02, 0x95, 0xb0, 0x05];
/// Magical version bytes for Zpub: bitcoin mainnet public key for
/// multi-signature P2WSH
pub const VERSION_MAGIC_ZPUB_MULTISIG: [u8; 4] = [0x02, 0xaa, 0x7e, 0xd3];
/// Magical version bytes for Zprv: bitcoin mainnet private key for
/// multi-signature P2WSH
pub const VERSION_MAGIC_ZPRV_MULTISIG: [u8; 4] = [0x02, 0xaa, 0x7a, 0x99];

/// Magical version bytes for tpub: bitcoin testnet/regtest public key for
/// P2PKH or P2SH
pub const VERSION_MAGIC_TPUB: [u8; 4] = [0x04, 0x35, 0x87, 0xCF];
/// Magical version bytes for tprv: bitcoin testnet/regtest private key for
/// P2PKH or P2SH
pub const VERSION_MAGIC_TPRV: [u8; 4] = [0x04, 0x35, 0x83, 0x94];
/// Magical version bytes for upub: bitcoin testnet/regtest public key for
/// P2WPKH in P2SH
pub const VERSION_MAGIC_UPUB: [u8; 4] = [0x04, 0x4A, 0x52, 0x62];
/// Magical version bytes for uprv: bitcoin testnet/regtest private key for
/// P2WPKH in P2SH
pub const VERSION_MAGIC_UPRV: [u8; 4] = [0x04, 0x4A, 0x4E, 0x28];
/// Magical version bytes for vpub: bitcoin testnet/regtest public key for
/// P2WPKH
pub const VERSION_MAGIC_VPUB: [u8; 4] = [0x04, 0x5F, 0x1C, 0xF6];
/// Magical version bytes for vprv: bitcoin testnet/regtest private key for
/// P2WPKH
pub const VERSION_MAGIC_VPRV: [u8; 4] = [0x04, 0x5F, 0x18, 0xBC];
/// Magical version bytes for Upub: bitcoin testnet/regtest public key for
/// multi-signature P2WSH in P2SH
pub const VERSION_MAGIC_UPUB_MULTISIG: [u8; 4] = [0x02, 0x42, 0x89, 0xef];
/// Magical version bytes for Uprv: bitcoin testnet/regtest private key for
/// multi-signature P2WSH in P2SH
pub const VERSION_MAGIC_UPRV_MULTISIG: [u8; 4] = [0x02, 0x42, 0x85, 0xb5];
/// Magical version bytes for Zpub: bitcoin testnet/regtest public key for
/// multi-signature P2WSH
pub const VERSION_MAGIC_VPUB_MULTISIG: [u8; 4] = [0x02, 0x57, 0x54, 0x83];
/// Magical version bytes for Zprv: bitcoin testnet/regtest private key for
/// multi-signature P2WSH
pub const VERSION_MAGIC_VPRV_MULTISIG: [u8; 4] = [0x02, 0x57, 0x50, 0x48];

/// Extended public and private key processing errors
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Error {
    /// error in BASE58 key encoding. Details: {0}
    Base58(base58::Error),

    /// error in hex key encoding. Details: {0}
    Hex(bitcoin::hashes::hex::HexToArrayError),

    /// pk->pk derivation was attempted on a hardened key.
    CannotDeriveFromHardenedKey,

    /// child number {0} is out of range.
    InvalidChildNumber(u32),

    /// invalid child number format.
    InvalidChildNumberFormat,

    /// invalid derivation path format.
    InvalidDerivationPathFormat,

    /// unknown version magic bytes {0:#06X?}
    UnknownVersion([u8; 4]),

    /// encoded extended key data has wrong length {0}
    WrongExtendedKeyLength(usize),

    /// unrecognized or unsupported extended key prefix (please check SLIP 32
    /// for possible values)
    UnknownSlip32Prefix,

    /// failure in rust bitcoin library
    InternalFailure,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Base58(e) => write!(f, "base58 error: {}", e),
            Error::Hex(e) => write!(f, "hex error: {}", e),
            Error::CannotDeriveFromHardenedKey => write!(f, "cannot derive from hardened key"),
            Error::InvalidChildNumber(n) => write!(f, "child number {} is out of range", n),
            Error::InvalidChildNumberFormat => write!(f, "invalid child number format"),
            Error::InvalidDerivationPathFormat => write!(f, "invalid derivation path format"),
            Error::UnknownVersion(v) => write!(f, "unknown version magic bytes {:02X?}", v),
            Error::WrongExtendedKeyLength(l) => write!(f, "wrong extended key length: {}", l),
            Error::UnknownSlip32Prefix => write!(f, "unknown SLIP32 prefix"),
            Error::InternalFailure => write!(f, "internal failure"),
        }
    }
}

impl std::error::Error for Error {}

impl From<bip32::Error> for Error {
    fn from(err: bip32::Error) -> Self {
        match err {
            bip32::Error::CannotDeriveFromHardenedKey => Error::CannotDeriveFromHardenedKey,
            bip32::Error::InvalidChildNumber(no) => Error::InvalidChildNumber(no),
            bip32::Error::InvalidChildNumberFormat => Error::InvalidChildNumberFormat,
            bip32::Error::InvalidDerivationPathFormat => Error::InvalidDerivationPathFormat,
            bip32::Error::Secp256k1(_) => Error::InternalFailure,
            bip32::Error::UnknownVersion(ver) => Error::UnknownVersion(ver),
            bip32::Error::WrongExtendedKeyLength(len) => Error::WrongExtendedKeyLength(len),
            bip32::Error::Base58(err) => Error::Base58(err),
            bip32::Error::Hex(err) => Error::Hex(err),
            _ => Error::InternalFailure,
        }
    }
}

impl From<base58::Error> for Error {
    fn from(err: base58::Error) -> Self {
        Error::Base58(err)
    }
}

/// Trait for building standard BIP32 extended keys from SLIP132 variant.
pub trait FromSlip132 {
    /// Constructs standard BIP32 extended key from SLIP132 string.
    fn from_slip132_str(s: &str) -> Result<Self, Error>
    where
        Self: Sized;
}

impl FromSlip132 for Xpub {
    fn from_slip132_str(s: &str) -> Result<Self, Error> {
        let mut data = base58::decode_check(s)?;

        let mut prefix = [0u8; 4];
        prefix.copy_from_slice(&data[0..4]);
        let slice = match prefix {
            VERSION_MAGIC_XPUB
            | VERSION_MAGIC_YPUB
            | VERSION_MAGIC_ZPUB
            | VERSION_MAGIC_YPUB_MULTISIG
            | VERSION_MAGIC_ZPUB_MULTISIG => VERSION_MAGIC_XPUB,

            VERSION_MAGIC_TPUB
            | VERSION_MAGIC_UPUB
            | VERSION_MAGIC_VPUB
            | VERSION_MAGIC_UPUB_MULTISIG
            | VERSION_MAGIC_VPUB_MULTISIG => VERSION_MAGIC_TPUB,

            _ => return Err(Error::UnknownSlip32Prefix),
        };
        data[0..4].copy_from_slice(&slice);

        let xpub = Xpub::decode(&data)?;

        Ok(xpub)
    }
}

impl FromSlip132 for Xpriv {
    fn from_slip132_str(s: &str) -> Result<Self, Error> {
        let mut data = base58::decode_check(s)?;

        let mut prefix = [0u8; 4];
        prefix.copy_from_slice(&data[0..4]);
        let slice = match prefix {
            VERSION_MAGIC_XPRV
            | VERSION_MAGIC_YPRV
            | VERSION_MAGIC_ZPRV
            | VERSION_MAGIC_YPRV_MULTISIG
            | VERSION_MAGIC_ZPRV_MULTISIG => VERSION_MAGIC_XPRV,

            VERSION_MAGIC_TPRV
            | VERSION_MAGIC_UPRV
            | VERSION_MAGIC_VPRV
            | VERSION_MAGIC_UPRV_MULTISIG
            | VERSION_MAGIC_VPRV_MULTISIG => VERSION_MAGIC_TPRV,

            _ => return Err(Error::UnknownSlip32Prefix),
        };
        data[0..4].copy_from_slice(&slice);

        let xprv = Xpriv::decode(&data)?;

        Ok(xprv)
    }
}
