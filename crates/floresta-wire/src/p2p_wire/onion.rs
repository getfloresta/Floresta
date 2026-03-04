use core::fmt;
use core::fmt::Debug;
use core::fmt::Display;
use core::fmt::Formatter;
use std::io::Write;
use std::str::FromStr;

use bitcoin::p2p::address::AddrV2;
use floresta_common::impl_error_from;
use sha3::Digest;
use sha3::Sha3_256;

/// The base32 alphabet.
///
/// These are the characters used while encoding/decoding a base32 payload, it intentionally
/// removes some characters that looks like other (like "1" could be confused with "l" or "I") in
/// some fonts. Each character represents one number between zero ("a") and 32 ("7").
const BASE32_ALPHABET: [char; 32] = [
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', '2', '3', '4', '5', '6', '7',
];

/// The suffix that comes at the end of all onion addresses.
///
/// This is technically the TLD used for onion addresses, and it's used by the tor resolver to
/// know it should open an internal circuit.
const ONION_SUFFIX: &str = ".onion";

/// The current encoding version as defined by the onion protocol V3
const ONION_ENCODING_VERSION: OnionVersion = OnionVersion(3);

/// How many bytes are in a pubkey
const PUBKEY_LENGTH: usize = 32;

/// How may bytes are in a checksum
const CHECKSUM_LENGTH: usize = 2;

/// The length of a chunk in bytes
const CHUNK_LENGTH_BYTES: usize = 5;

// === Wrapper types ===

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
/// A wrapper around the version used by the onion protocol
///
/// This version is encoded within the onion address and will be used to make future changes in a
/// backwards-compatible way. Currently, only three is defined.
pub struct OnionVersion(u8);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
/// A wrapper for the checksum used in a onion address
///
/// Every address has a checksum to protect against typos or non-intentional corruption. It is
/// defined as sha3-256(".onion checksum" | onion_service_key || onion_version)[0..2]
pub struct OnionChecksum([u8; CHECKSUM_LENGTH]);

impl From<[u8; CHECKSUM_LENGTH]> for OnionChecksum {
    fn from(value: [u8; CHECKSUM_LENGTH]) -> Self {
        Self(value)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
/// The public key of an onion service.
///
/// This is the most important part in an onion address
pub struct OnionPubkey([u8; PUBKEY_LENGTH]);

// === Error types ===

#[derive(Debug, PartialEq, Eq, PartialOrd, Clone, Copy)]
/// A zero-size error type for fallible conversions where the only possible error is invalid size
///
/// We use this mostly when extracting bytes from a byte-slice to build one of our inner wrapper
/// types.
pub struct InvalidLength;

#[derive(Debug)]
/// A zero-size error returned when there's an error converting from [`AddrV2`]
///
/// This error can only mean one thing: you've called .try_from in an [`AddrV2`] address that isn't a
/// [`AddrV2::TorV3`] address.
pub struct FromAddrV2Error;

#[derive(Debug, PartialEq, Eq)]
/// Error returned when we can't decode an onion address string
pub enum OnionAddressDecodeError {
    /// Something went wrong while decoding the base32 payload, like there's an invalid char.
    Base32(Base32DecodeError),

    /// This address doesn't have the `.onion` TLD.
    MissingDotOnion,

    /// The computed checksum didn't match what was provided by this address
    InvalidChecksum,

    /// The provided onion version is unsupported by this implementation
    InvalidOnionVersion,

    /// We got a string with the wrong length for an onion address
    InvalidLength,
}

#[derive(Debug, PartialEq, Eq)]
/// An error returned by our base32 decode logic.
pub enum Base32DecodeError {
    /// The provided string's length is invalid for base32
    InvalidLength,

    /// Non-base32 character found in your string
    InvalidBase32Character { invalid_character: char },

    /// The string passed-in isn't ascii
    NotAsciiString,
}

impl_error_from!(OnionAddressDecodeError, Base32DecodeError, Base32);

// === Trait Impls ===

impl TryFrom<&[u8]> for OnionPubkey {
    type Error = InvalidLength;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != PUBKEY_LENGTH {
            return Err(InvalidLength);
        }

        let mut inner_array = [0; PUBKEY_LENGTH];
        inner_array.copy_from_slice(value);

        Ok(Self(inner_array))
    }
}

impl TryFrom<&[u8]> for OnionChecksum {
    type Error = InvalidLength;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 2 {
            return Err(InvalidLength);
        }

        let mut inner_array = [0; CHECKSUM_LENGTH];
        inner_array.copy_from_slice(value);

        Ok(Self(inner_array))
    }
}

impl TryFrom<&[u8]> for OnionVersion {
    type Error = InvalidLength;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != u8::BITS as usize {
            return Err(InvalidLength);
        }

        let version = value[0];
        Ok(Self(version))
    }
}

impl TryFrom<AddrV2> for OnionV3Addr {
    type Error = FromAddrV2Error;

    fn try_from(value: AddrV2) -> Result<Self, Self::Error> {
        let AddrV2::TorV3(key) = value else {
            return Err(FromAddrV2Error);
        };

        let pubkey: OnionPubkey =
            OnionPubkey::try_from(key.as_slice()).expect("This is always 32-bytes");

        Ok(Self {
            pubkey,
            version: ONION_ENCODING_VERSION,
        })
    }
}

impl TryFrom<&[u8]> for OnionV3Addr {
    type Error = InvalidLength;

    fn try_from(key: &[u8]) -> Result<Self, Self::Error> {
        let pubkey: OnionPubkey = OnionPubkey::try_from(key)?;

        Ok(Self {
            pubkey,
            version: ONION_ENCODING_VERSION,
        })
    }
}

impl From<[u8; PUBKEY_LENGTH]> for OnionPubkey {
    fn from(value: [u8; PUBKEY_LENGTH]) -> Self {
        Self(value)
    }
}

impl From<[u8; PUBKEY_LENGTH]> for OnionV3Addr {
    fn from(value: [u8; PUBKEY_LENGTH]) -> Self {
        let pubkey: OnionPubkey = value.into();

        Self {
            pubkey,
            version: ONION_ENCODING_VERSION,
        }
    }
}

impl FromStr for OnionV3Addr {
    type Err = OnionAddressDecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::onion_address_from_b32(s)
    }
}

impl Display for OnionV3Addr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_human_readable())
    }
}

// === The actual onion address impl ===

#[derive(PartialEq, Eq)]
/// A Tor Onion Service address, following the latest protocol version (three).
///
/// This is our internal representation for the famous Tor Onion Services address, that usually
/// has several random-looking characters, ending with `.onion`. They represent the information
/// needed to contact a Onion Service, like their public key and which protocol version they speak.
/// It also has a small checksum to catch potential typos.
///
/// We use this internally to connect with onion services. The Tor daemon requires a human-readable
/// Onion Address, while Bitcoin's [`AddrV2`] gives you an opaque 32-bytes array. This type allows
/// us to go back-and-forth and use whichever representation we need.
pub struct OnionV3Addr {
    /// The x-coordinate of the Onion Service's ed25519 key.
    ///
    /// This is the heart of an onion address, since this will be both used to locate the service
    /// within the network, and will also be used to authenticate that you are actually talking to
    /// the part you intended.
    ///
    /// This is a 32-bytes, and encodes that server's ed25519 long-term key
    pubkey: OnionPubkey,

    /// The introduction protocol version we should try to use when talking to this service
    version: OnionVersion,
}

impl Debug for OnionV3Addr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_human_readable())
    }
}

impl OnionV3Addr {
    /// Computes the checksum for a Onion Address
    ///
    /// These are currently defined as the SHA3-256 of:
    ///   - ".onion checksum"
    ///   - the onion server's ed25519 pubkey X-coordinate
    ///   - The introduction protocol version, currently "3"
    ///
    /// These should be serialized as a byte-array and hashed. The checksum will be resulting
    /// hash's two lowest bytes.
    pub fn checksum(&self) -> OnionChecksum {
        let mut hash = Sha3_256::new();

        // to compute the checksum, we must write...

        // ... ".onion checksum" as bytes...
        hash.write_all(".onion checksum".as_bytes())
            .expect("in-memory hashers don't err");

        // ... the server pubkey bytes ...
        hash.write_all(&self.pubkey.0)
            .expect("in-memory hashers don't err");

        // ... and finally, the protocol version as bytes ...
        hash.write_all(&self.version.0.to_be_bytes())
            .expect("in-memory hashers don't err");

        let digest = hash.finalize();
        let mut final_checksum = [0; CHECKSUM_LENGTH];
        final_checksum.copy_from_slice(&digest[0..CHECKSUM_LENGTH]);

        final_checksum.into()
    }

    /// Produces a human-readable version of an onion address
    ///
    /// This will take a raw onion address and turn it into the familiar `.onion` address
    pub fn as_human_readable(&self) -> String {
        let mut address = self.b32_encode();
        address.push_str(ONION_SUFFIX);

        address
    }

    /// Decodes a [base32] encoded chunk of data
    ///
    /// base32 works by dividing the original buffer into groups of 5 bytes (40 bits),
    /// then further chopping it into subgroups of 5 bits, from the MSB to the LSB. Each
    /// subgroup is used to index into an alphabet of 32 symbols.
    ///
    /// Before doing the first division, the data must be padded until the size in bytes is a
    /// multiple of 5
    ///
    /// For example: foo in binary is 0x666f6f. We must pad it by adding two zero-bytes:
    /// 0x666f6f0000
    ///
    /// If we look at the binary representation, we have: 0110 0110 0110 1111 0110 1111 0000 0000
    /// regrouping into chunks of 5: 01100 11001 10111 10110 11110 0000000
    ///
    /// In decimal: 12 25 23 22 30
    /// If we look this up in the table: `mzxw6`. We can cross-check this with the RFC and note that
    /// they have the same value, with the addition of a few equals sign (=), this is for padding!
    ///
    /// [base32]: https://www.rfc-editor.org/rfc/rfc4648
    fn b32_encode(&self) -> String {
        let mut final_str = String::new();
        let mut writer = Vec::new();

        writer
            .write_all(self.pubkey.0.as_slice())
            .expect("in-memory writers don't err");

        writer
            .write_all(self.checksum().0.as_slice())
            .expect("in-memory writers don't err");

        writer
            .write_all(&self.version.0.to_be_bytes())
            .expect("in-memory writers don't err");

        for chunk in writer.chunks(CHUNK_LENGTH_BYTES) {
            // note: we can hard-code 40 here because we have 35 bytes, which is multiple of 5
            final_str.push_str(&Self::b32_encode_chunk(chunk, 40));
        }

        final_str
    }

    /// Process a single base32 chunk.
    ///
    /// When encoding an array of bytes into a base32 encoding we break our data into
    /// chunks of 5 bytes, then encode each chunk individually. This method will do that.
    ///
    /// The `total` parameter tells how many non-padding bytes we have. If you are encoding
    /// the message "foo", this will have three bytes, so total = 3. Callers are responsible for
    /// adding two equals signs for padding purposes. This function will only return the equivalent
    /// of those three bytes and ignore the two padding bytes. If you pass five as `total` in the
    /// previous example, you will end up with some leading `a` characters in your encoded value.
    fn b32_encode_chunk(chunk: &[u8], mut total: u8) -> String {
        // A mask used to extract the bits we are working with
        const MASK: u64 = 0b11111;

        // The total size of a u64
        const PADDED_CHUNK_SIZE: usize = 8;

        // This will be used to construct a u64
        let mut padded_chunk = [0; PADDED_CHUNK_SIZE];
        padded_chunk[0..CHUNK_LENGTH_BYTES].copy_from_slice(chunk);

        let mut chunk_encoded = String::new();
        let chunk_as_u64 = u64::from_be_bytes(padded_chunk);
        let mut offset = u64::BITS as usize - CHUNK_LENGTH_BYTES;

        while total > 0 {
            // This ugly predicate will extract a sequence of five bits from the u64
            let remainder = ((chunk_as_u64 & (MASK << offset)) >> (offset)) as u8;

            offset = offset.saturating_sub(5);
            total = total.saturating_sub(5);

            let ch = BASE32_ALPHABET[remainder as usize];
            chunk_encoded.push(ch);
        }

        chunk_encoded
    }

    /// Returns a numerical value associated with a base32 char
    fn b32_get_char_value(ch: char) -> Option<usize> {
        BASE32_ALPHABET.iter().position(|_ch| *_ch == ch)
    }

    /// Decode a chunk of 8 base32 characters
    fn b32_decode_chunk(chunk: &str) -> Result<[u8; 5], Base32DecodeError> {
        let mut acc = 0_u64;
        let mut offset = 64 - 5;
        for ch in chunk.chars() {
            if !ch.is_ascii() {
                return Err(Base32DecodeError::InvalidBase32Character {
                    invalid_character: ch,
                });
            }

            let val =
                Self::b32_get_char_value(ch).ok_or(Base32DecodeError::InvalidBase32Character {
                    invalid_character: ch,
                })?;

            acc |= (val << offset) as u64;
            offset -= 5;
        }

        let mut result = [0; 5];
        let acc_bytes = acc.to_be_bytes();
        result.copy_from_slice(&acc_bytes[0..5]);

        Ok(result)
    }

    /// Decodes a base32 payload, returning an array of bytes
    fn b32_decode(value: &str) -> Result<Vec<u8>, Base32DecodeError> {
        const B32_STRING_CHUNK_SIZE: usize = 8;

        if !value.is_ascii() {
            return Err(Base32DecodeError::NotAsciiString);
        }

        if value.is_empty() || value.len() % B32_STRING_CHUNK_SIZE != 0 {
            return Err(Base32DecodeError::InvalidLength);
        }

        let mut final_vec = Vec::new();
        let chunks = value.len() / B32_STRING_CHUNK_SIZE;

        for chunk in 0..chunks {
            let lo = chunk * B32_STRING_CHUNK_SIZE;
            let hi = (chunk * B32_STRING_CHUNK_SIZE) + B32_STRING_CHUNK_SIZE;
            let chunk = &value[lo..hi];

            let decoded = Self::b32_decode_chunk(chunk)?;
            final_vec.extend(decoded);
        }

        Ok(final_vec)
    }

    /// Parses an onion address and returns [OnionV3Addr]
    fn onion_address_from_b32(address: &str) -> Result<Self, OnionAddressDecodeError> {
        if !address.ends_with(".onion") {
            return Err(OnionAddressDecodeError::MissingDotOnion);
        }

        // remove the `.onion` part
        let address_part = address.replace(".onion", "");
        let decoded_bytes = Self::b32_decode(&address_part)?;

        if decoded_bytes.len() != 35 {
            return Err(OnionAddressDecodeError::InvalidLength);
        }

        let pubkey: OnionPubkey = decoded_bytes[0..32]
            .try_into()
            .expect("must have 32 bytes left, we've checked it");

        let checksum: OnionChecksum = decoded_bytes[32..34]
            .try_into()
            .expect("must have two bytes left, we've checked it");

        let version: OnionVersion = OnionVersion(decoded_bytes[34]);
        if version != ONION_ENCODING_VERSION {
            return Err(OnionAddressDecodeError::InvalidOnionVersion);
        }

        let onion_address = Self { pubkey, version };
        let computed_checksum = onion_address.checksum();

        if computed_checksum != checksum {
            return Err(OnionAddressDecodeError::InvalidChecksum);
        }

        Ok(onion_address)
    }

    /// Returns the serialized internal pubkey
    pub fn into_bytes(self) -> [u8; PUBKEY_LENGTH] {
        self.pubkey.0
    }
}

#[cfg(test)]
pub mod tests {
    use bitcoin::p2p::address::AddrV2;

    use crate::p2p_wire::onion::Base32DecodeError;
    use crate::p2p_wire::onion::OnionAddressDecodeError;
    use crate::p2p_wire::onion::OnionV3Addr;

    /// An onion address without the `.onion` part
    const ONION_ADDRESS_PAYLOAD_ONLY: &str =
        "ejgeimjypsfuijpxzy5xpwmmjmkr4izwze6od5pw74csjglflib6nsid";

    /// A full onion address, with the `.onion`
    const ONION_ADDRESS: &str = "ejgeimjypsfuijpxzy5xpwmmjmkr4izwze6od5pw74csjglflib6nsid.onion";

    /// A serialized ed25519 x-coordinate associated with an onion address
    const ONION_ADDRESS_KEY_BYTES: [u8; 32] = [
        34, 76, 68, 49, 56, 124, 139, 68, 37, 247, 206, 59, 119, 217, 140, 75, 21, 30, 35, 54, 201,
        60, 225, 245, 246, 255, 5, 36, 153, 101, 90, 3,
    ];

    #[test]
    fn test_process_chunk() {
        let chunk = [0x66, 0x6f, 0x6f, 0x00, 0x00];
        let computed = OnionV3Addr::b32_encode_chunk(&chunk, 3 * 8);

        assert_eq!(computed, String::from("mzxw6"));
    }

    #[test]
    fn test_parse_chunk() {
        let chunk = "mzxw6";
        let processed = OnionV3Addr::b32_decode_chunk(chunk).unwrap();
        assert_eq!(processed, [0x66, 0x6f, 0x6f, 0x00, 0x00]);
    }

    #[test]
    fn test_from_addrv2() {
        let address = AddrV2::TorV3(ONION_ADDRESS_KEY_BYTES);
        let address = OnionV3Addr::try_from(address).unwrap();

        assert_eq!(
            address.b32_encode(),
            String::from(ONION_ADDRESS_PAYLOAD_ONLY)
        );
    }

    #[test]
    fn test_as_human_readable() {
        let address: OnionV3Addr = ONION_ADDRESS_KEY_BYTES.into();

        assert_eq!(address.as_human_readable(), String::from(ONION_ADDRESS));
    }

    #[test]
    fn test_parse_onion_address() {
        let original = String::from(ONION_ADDRESS_PAYLOAD_ONLY);
        let expected_decoding = ONION_ADDRESS_KEY_BYTES;

        let decoded = &OnionV3Addr::b32_decode(&original).unwrap()[0..32];
        assert_eq!(decoded, expected_decoding);
    }

    #[test]
    fn onion_address_rtt() {
        let original = String::from(ONION_ADDRESS);

        let parsed_address: OnionV3Addr = original.parse().unwrap();

        let reencoded = parsed_address.as_human_readable();
        assert_eq!(reencoded, original);
    }

    #[test]
    fn test_onion_invalid_length() {
        let original = String::from(ONION_ADDRESS);

        // multiple of 8, valid base32 but not valid onion address
        let invalid = &original[8..];
        let err = invalid.parse::<OnionV3Addr>().unwrap_err();
        assert_eq!(err, OnionAddressDecodeError::InvalidLength);

        // not a multiple of 8, invalid in both cases (but base32 decode catches it first)
        let invalid = &original[1..];
        let err = invalid.parse::<OnionV3Addr>().unwrap_err();
        assert_eq!(
            err,
            OnionAddressDecodeError::Base32(Base32DecodeError::InvalidLength)
        );
    }

    #[test]
    fn test_missing_dot_onion() {
        let err = ONION_ADDRESS_PAYLOAD_ONLY
            .parse::<OnionV3Addr>()
            .unwrap_err();

        assert_eq!(err, OnionAddressDecodeError::MissingDotOnion);
    }

    #[test]
    fn test_bad_checksum() {
        let mut address = String::from(ONION_ADDRESS);
        // was `i`, now it's `z`
        address.replace_range(0..1, "z");

        let err = address.parse::<OnionV3Addr>().unwrap_err();
        assert_eq!(err, OnionAddressDecodeError::InvalidChecksum);
    }

    #[test]
    fn test_bad_version() {
        let mut address = String::from(ONION_ADDRESS);
        // 4 instead of 3
        address.replace_range(55..56, "e");

        let err = address.parse::<OnionV3Addr>().unwrap_err();
        assert_eq!(err, OnionAddressDecodeError::InvalidOnionVersion);
    }

    #[test]
    fn test_bad_b32_encoding() {
        let mut address = String::from(ONION_ADDRESS);
        // add a 8, which is not base32
        address.replace_range(0..1, "8");

        let err = address.parse::<OnionV3Addr>().unwrap_err();
        assert_eq!(
            err,
            OnionAddressDecodeError::Base32(Base32DecodeError::InvalidBase32Character {
                invalid_character: '8'
            })
        );
    }

    #[test]
    fn test_non_ascii() {
        let address = [
            111, 105, 110, 110, 110, 110, 242, 150, 144, 142, 110, 110, 110, 110, 110, 110, 110,
            110, 110, 111, 105, 111, 110, 111, 111, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 110, 110, 110, 110, 110, 110, 110, 110, 110, 63, 110,
            110, 110, 46, 110, 110, 110, 110, 111, 111, 105, 110, 111, 111, 110, 110, 110, 110,
            110, 110, 110, 110, 110, 63, 110, 110, 110, 46, 111, 110, 105, 111, 110,
        ];

        let address = std::str::from_utf8(&address).unwrap();
        let address = address.parse::<OnionV3Addr>().unwrap_err();

        assert_eq!(
            address,
            OnionAddressDecodeError::Base32(Base32DecodeError::NotAsciiString)
        );
    }
}
