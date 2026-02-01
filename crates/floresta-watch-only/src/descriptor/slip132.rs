// Based on slip132 from LNP/BP Descriptor Wallet library by:
//     Dr. Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Adapted for Floresta by:
//     Davidson Sousa <me@dlsouza.lol>

//! Bitcoin SLIP-132 standard implementation for parsing custom xpub/xpriv key
//! formats

use core::fmt;
use core::fmt::Debug;

use bitcoin::base58;
use bitcoin::bip32;
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

    /// No support for xpriv
    NoSupportXpriv,

    /// No multisig support via xpub.
    NoSupportXpubMultisig,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Base58(e) => write!(f, "Base58 error: {}", e),
            Error::Hex(e) => write!(f, "Hex error: {}", e),
            Error::CannotDeriveFromHardenedKey => {
                write!(f, "Cannot derive from hardened key")
            }
            Error::InvalidChildNumber(no) => write!(f, "Invalid child number: {}", no),
            Error::InvalidChildNumberFormat => write!(f, "Invalid child number format"),
            Error::InvalidDerivationPathFormat => write!(f, "Invalid derivation path format"),
            Error::UnknownVersion(ver) => write!(f, "Unknown version bytes: {:02X?}", ver),
            Error::WrongExtendedKeyLength(len) => {
                write!(f, "Wrong extended key length: {}", len)
            }
            Error::UnknownSlip32Prefix => write!(f, "Unknown SLIP-132 prefix"),
            Error::InternalFailure => write!(f, "Internal failure"),
            Error::NoSupportXpriv => write!(f, "No support for xpriv keys"),
            Error::NoSupportXpubMultisig => write!(f, "No support for xpub multisig keys"),
        }
    }
}

fn extract_slip132_prefix(s: &str) -> Result<[u8; 4], Error> {
    let data = base58::decode_check(s)?;
    let mut prefix = [0u8; 4];
    prefix.copy_from_slice(&data[0..4]);

    validate_slip132_prefix(prefix)?;

    Ok(prefix)
}

fn validate_slip132_prefix(prefix: [u8; 4]) -> Result<(), Error> {
    match prefix {
        VERSION_MAGIC_XPUB | VERSION_MAGIC_YPUB | VERSION_MAGIC_ZPUB | VERSION_MAGIC_TPUB
        | VERSION_MAGIC_UPUB | VERSION_MAGIC_VPUB => Ok(()),

        VERSION_MAGIC_XPRV | VERSION_MAGIC_YPRV | VERSION_MAGIC_ZPRV | VERSION_MAGIC_TPRV
        | VERSION_MAGIC_UPRV | VERSION_MAGIC_VPRV => Err(Error::NoSupportXpriv),

        VERSION_MAGIC_YPUB_MULTISIG
        | VERSION_MAGIC_ZPUB_MULTISIG
        | VERSION_MAGIC_UPUB_MULTISIG
        | VERSION_MAGIC_VPUB_MULTISIG
        | VERSION_MAGIC_YPRV_MULTISIG
        | VERSION_MAGIC_ZPRV_MULTISIG
        | VERSION_MAGIC_UPRV_MULTISIG
        | VERSION_MAGIC_VPRV_MULTISIG => Err(Error::NoSupportXpubMultisig),

        _ => Err(Error::UnknownSlip32Prefix),
    }
}

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

        let prefix: [u8; 4] = extract_slip132_prefix(s)?;
        let slice = match prefix {
            VERSION_MAGIC_XPUB | VERSION_MAGIC_YPUB | VERSION_MAGIC_ZPUB => VERSION_MAGIC_XPUB,

            VERSION_MAGIC_TPUB | VERSION_MAGIC_UPUB | VERSION_MAGIC_VPUB => VERSION_MAGIC_TPUB,

            _ => return Err(Error::UnknownSlip32Prefix),
        };
        data[0..4].copy_from_slice(&slice);

        let xpub = Xpub::decode(&data)?;

        Ok(xpub)
    }
}

/// Generates a descriptor based on the provided xpub.
/// The descriptor type is determined by the xpub's prefix:
/// - P2PKH for xpub/tpub (Legacy addresses)
/// - P2WPKH-P2SH for ypub/upub (SegWit nested in P2SH)
/// - P2WPKH for zpub/vpub (Native SegWit)
pub fn generate_descriptor_from_xpub(s: &str, change: bool) -> Result<String, Error> {
    let index = if change { 1 } else { 0 };
    let xpub = Xpub::from_slip132_str(s)?;

    let prefix = extract_slip132_prefix(s)?;

    match prefix {
        VERSION_MAGIC_XPUB | VERSION_MAGIC_TPUB => Ok(format!("pkh({xpub}/{index}/*)")),
        VERSION_MAGIC_YPUB | VERSION_MAGIC_UPUB => Ok(format!("sh(wpkh({xpub}/{index}/*))")),
        VERSION_MAGIC_ZPUB | VERSION_MAGIC_VPUB => Ok(format!("wpkh({xpub}/{index}/*)")),

        _ => Err(Error::UnknownSlip32Prefix),
    }
}

/// Checks if the xpub belongs to the mainnet based on its prefix.
pub fn is_xpub_mainnet(s: &str) -> Result<bool, Error> {
    let prefix = extract_slip132_prefix(s)?;
    match prefix {
        VERSION_MAGIC_XPUB | VERSION_MAGIC_YPUB | VERSION_MAGIC_ZPUB => Ok(true),

        VERSION_MAGIC_TPUB | VERSION_MAGIC_UPUB | VERSION_MAGIC_VPUB => Ok(false),

        _ => Err(Error::UnknownSlip32Prefix),
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const XPUB: &str = "xpub6CPimhNogJosVzpueNmrWEfSHc2YTXG1ZyE6TBV4Nx6UxZ7zKSGYv9hKxNjiFY5o1vz7QeZa2m6vQmyndDrkECk8cShWYWxe1gqa1xJEkgs";
    const XPRIV: &str = "xprv9yQNNBquqwFaHWkSYMEr96ihjaC444YACkJVeo5SpcZW5knqmtxJNMNr781jwtB2PRV1BJMdwAMrQt7DHUumiA7h1BGp2C4h2C1geFtYMzs";
    const YPUB: &str = "ypub6XmBfjfmuYD1bjv5RCEHU8jD1NPGZh6NRTGDB8ndQsd7MPnzhDhAsdrF9sK8Z4G9FvcFBHoGsZqhsDHtenca3K5QigYWVKXvkAx6HBxVGYM";
    const YPRIV: &str = "yprvAJmqGE8t5AeiPFqcKAhH6znUTLYnAENX4ELcNkP1rY68UbTr9gNvKqXmJZe1RsaVZquKb9UR2KkwUTGcN337oFKkyqFRmPKhpdcLLiVQMZ6";
    const ZPUB: &str = "zpub6rFvSvP5VbpXwej2L5WseLfxfdUzSczs9DK9v9mpXgXNqjFhtfUTRGkQKr7sXKNyrrzhd2LCysGqts1oT3b1PJji16xWzcmNMfhmZ8kkLZ1";
    const ZPRIV: &str = "zprvAdGa3QrBfEGEjAeZE3ysHCjE7beW3AH1mzPZ7mNCyLzPxvvZM8ACsURvUbcnW5hV91XfdZNzi8jvPLZ4fubdG4qqKkFFojgd6JN67Hyy8xT";

    const TPUB: &str = "tpubDC73PMTHeKDXnFwNFz8CLBy2VVx4D85WW2vbzwVLwCD9zkQ6Vj97muhLRTbKvmue1PyVQLwizvBW6v2SD1LnzbeuHnRsDYQZGE8urTZHMn5";
    const TPRIV: &str = "tprv8fR1EwR3VwXrtnuaNLTbvnJuvUS83ntbvjKpiRT3WvQmAG9KsLKXbR5UFKPAiRJKyDxiLd2uovStSJ3Qnov7SgkKK4mUcghwX7KHwjhSEFi";
    const UPUB: &str = "upub5E3Vhaq9uVmz426B5FME1csAY8tvQ8vRqt7WnGyiJ4CoknpyM2WJk4B6uSh2kud3r8RJHTzS5jLFnWNRThKZyew6tDX2eXGMyTvfa8AVwyK";
    const UPRIV: &str = "uprv9149J5JG58DgqY1hyDpDeUvRz74RzgCaUfBuyta6jifpszVpoVC4CFrd4D7E5QFmzdYJ1EuLFq9Ge4TvCAymG8cPt3QLz7UJ8Fpsiwgg7Lg";
    const VPUB: &str = "vpub5Zrsj9pYeJLwTfggbSQYZDdpEpZ4M1qB1EUKfXB9bjsookSNjM6c6eFTYfjb8KcGJV4ZqAYScBvC7hyDbbWKCHVcC6RETNJUfwUFvnHJM8Y";
    const VPRIV: &str = "vprv9LsXKeHeovneFBcDVQsYC5h5gniZwZ7Ke1Yis8mY3QLpvx7EBonMYqvyhMwbmAfR5SXLA2byEyPbB3uAajJhAr2NNeM91WfAwRS1KcjCeFo";

    const YPRIV_MULTISIG: &str = "YprvARd5Qbw8ES2RHXALKfJ8v2hnKe3iDZNuG6kupDzPxXJicdKJ4Mc3TN8D1GNHsQsNTS7wGPts88TJ8pjKKCYLon9GopLzQ6fr9VcTWRUxbLq";
    const YPUB_MULTISIG: &str = "Ypub6ecRp7U24oaiW1EoRgq9HAeWsftCd26kdKgWccQ1WrqhVReSbtvJ1ASgrXLA4CSKw11yatzpmyYy3LraoW2E7kd7X32fTnwdqHnESpnyrKb";
    const ZPRIV_MULTISIG: &str = "ZprvAkTLiGc3P7Zu8pMTA25m87oHVcCAABNQBDH8bctHLXgbfj8XK1mc5RnM2UKssKXHs5Ek1sVRanor27Lt2txMc1psgA3Qz1VLRDg6u4gAyFo";
    const ZPUB_MULTISIG: &str = "Zpub6ySh7n8wDV8CMJRvG3cmVFk23e2eZe6FYSCjQ1HttsDaYXTfrZ5rdE6psjHk476FLe8nLNbPEduWvdU9XCSEuzJiPNj63hm871qsqUVx7kC";
    const UPRIV_MULTISIG: &str = "Uprv98J2BwFTdhrVtLPrzE9e5gKmdmTvT5Qubeg2geQrSVoCQE4P3iwny7VevSXwsnFgpseiGVWdHV36bgH4SQtHcqQsLTZJ4TPu4bMsx8v4vMj";
    const UPUB_MULTISIG: &str = "Upub5MHNbSnMU5Qo6pUL6FgeSpGWBoJQrY8kxsbdV2pTzqLBH2PXbGG3Wup8mhVp4ZpeJSYkazcawL8mWCQKviNAvoti3gEy89fgkPXetXJzNZt";
    const VPRIV_MULTSIG: &str = "Vprv19RbkhR2UPGc4edf2xogFVhtnjvvdpwMJgX4yug3xZufVdwzDXc1xUrQcnaQMZLgp3mPQVD5NQ9QFdwktTo1LgumfqiidCHCfTaY2MtjLrZ";
    const VPUB_MULTSIG: &str = "Vpub5g7du7TGckxGx7fSvcUGeuN1MmSroA8Fsz7rGRiMNqi4L8CkqvRc8yUGnuTQ4UUZi5fZLUD9PzVKPV1teQnBj3aJv1wPi4VB27bJH5b5suR";

    fn create_invalid_slip32_base58(reference: &str) -> String {
        let mut data = base58::decode_check(reference).unwrap();
        // Change prefix
        data[3] = 0x21;

        // Remove checksum
        data.truncate(data.len() - 4);

        base58::encode_check(&data)
    }

    #[test]
    fn test_check_unknown_slip32_prefix_error() {
        let cases = [
            create_invalid_slip32_base58(XPUB),
            create_invalid_slip32_base58(YPUB),
            create_invalid_slip32_base58(ZPUB),
            create_invalid_slip32_base58(TPUB),
            create_invalid_slip32_base58(UPUB),
            create_invalid_slip32_base58(VPUB),
        ];

        for key in cases.iter() {
            let result = extract_slip132_prefix(key);
            assert_eq!(result.err().unwrap(), Error::UnknownSlip32Prefix);
        }
    }

    #[test]
    fn test_verify_multisig_prefix_error() {
        let cases = &[
            YPRIV_MULTISIG,
            YPUB_MULTISIG,
            ZPRIV_MULTISIG,
            ZPUB_MULTISIG,
            UPRIV_MULTISIG,
            UPUB_MULTISIG,
            VPRIV_MULTSIG,
            VPUB_MULTSIG,
        ];
        for key in cases.iter() {
            let result = extract_slip132_prefix(key);
            assert_eq!(result.err().unwrap(), Error::NoSupportXpubMultisig);
        }
    }

    #[test]
    fn test_check_xpriv_support_error() {
        let cases = &[(XPRIV), (YPRIV), (ZPRIV), (TPRIV), (UPRIV), (VPRIV)];

        for key in cases.iter() {
            let result = extract_slip132_prefix(key);
            assert_eq!(result.err().unwrap(), Error::NoSupportXpriv);
        }
    }

    #[test]
    fn test_validate_network_xpub() {
        let cases = &[
            (XPUB, true),
            (YPUB, true),
            (ZPUB, true),
            (TPUB, false),
            (UPUB, false),
            (VPUB, false),
        ];

        for &(key, change) in cases.iter() {
            let result = is_xpub_mainnet(key);
            assert_eq!(result.unwrap(), change);
        }
    }

    #[test]
    fn test_descriptor_generation_for_xpub() {
        let cases: &[(&str, bool, &str); 12] = &[
            (XPUB, true, "pkh(xpub6CPimhNogJosVzpueNmrWEfSHc2YTXG1ZyE6TBV4Nx6UxZ7zKSGYv9hKxNjiFY5o1vz7QeZa2m6vQmyndDrkECk8cShWYWxe1gqa1xJEkgs/1/*)"),
            (XPUB, false, "pkh(xpub6CPimhNogJosVzpueNmrWEfSHc2YTXG1ZyE6TBV4Nx6UxZ7zKSGYv9hKxNjiFY5o1vz7QeZa2m6vQmyndDrkECk8cShWYWxe1gqa1xJEkgs/0/*)"),
            (YPUB, true, "sh(wpkh(xpub6CvvN4zrkrfXkSixaqSfG3dhqQEpd56sWLjzPjtk2sFEJHymSZXcFaC78fMYZ9cDrHVSRpCiQuV9yvgKw6CZF5PorLr5uQiSUStStZjpSSV/1/*))"),
            (YPUB, false, "sh(wpkh(xpub6CvvN4zrkrfXkSixaqSfG3dhqQEpd56sWLjzPjtk2sFEJHymSZXcFaC78fMYZ9cDrHVSRpCiQuV9yvgKw6CZF5PorLr5uQiSUStStZjpSSV/0/*))"),
            (ZPUB, true, "wpkh(xpub6CbPqb3FCEjaF4LnfMwdEAUxKhC6ZP1sJzGiMMz3mfmcjXdFPM9LB9S8HSChXW593am685964YZk8Hng1ekynqNWGRZfpo8PpDaUmyvQqvY/1/*)"),
            (ZPUB, false, "wpkh(xpub6CbPqb3FCEjaF4LnfMwdEAUxKhC6ZP1sJzGiMMz3mfmcjXdFPM9LB9S8HSChXW593am685964YZk8Hng1ekynqNWGRZfpo8PpDaUmyvQqvY/0/*)"),
            (TPUB, true, "pkh(tpubDC73PMTHeKDXnFwNFz8CLBy2VVx4D85WW2vbzwVLwCD9zkQ6Vj97muhLRTbKvmue1PyVQLwizvBW6v2SD1LnzbeuHnRsDYQZGE8urTZHMn5/1/*)"),
            (TPUB, false, "pkh(tpubDC73PMTHeKDXnFwNFz8CLBy2VVx4D85WW2vbzwVLwCD9zkQ6Vj97muhLRTbKvmue1PyVQLwizvBW6v2SD1LnzbeuHnRsDYQZGE8urTZHMn5/0/*)"),
            (UPUB, true, "sh(wpkh(tpubDCuv8pfb4pMsshrP2WhBqoV3PARvDPPz8rGUV1iWmz6LfNwNBDr5kgpMD6eaH8Y3rxJd9UHyzpDx8Yhj1eQrFoSCYqMc5nP4Nbi1VvJmNco/1/*))"),
            (UPUB, false, "sh(wpkh(tpubDCuv8pfb4pMsshrP2WhBqoV3PARvDPPz8rGUV1iWmz6LfNwNBDr5kgpMD6eaH8Y3rxJd9UHyzpDx8Yhj1eQrFoSCYqMc5nP4Nbi1VvJmNco/0/*))"),
            (VPUB, true , "wpkh(tpubDDu2riz4ewPMS4FmiLxtBKABuswcDeKEP674as24hfPTfEjYJtGpVDEZq7jYedsLufq5whFS4cTLaTgxRrBagCK6zNZPJibgoMBxTvUcVFf/1/*)"),
            (VPUB, false, "wpkh(tpubDDu2riz4ewPMS4FmiLxtBKABuswcDeKEP674as24hfPTfEjYJtGpVDEZq7jYedsLufq5whFS4cTLaTgxRrBagCK6zNZPJibgoMBxTvUcVFf/0/*)"),
        ];

        for &(key, change, expect) in cases.iter() {
            let result = generate_descriptor_from_xpub(key, change).unwrap();
            assert_eq!(result, expect);
        }
    }
}
