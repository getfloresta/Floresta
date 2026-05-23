// SPDX-License-Identifier: MIT OR Apache-2.0

//! This module defines the structure for JSON-RPC requests and provides utility functions to
//! extract parameters from the request.

use std::str::FromStr;

use bitcoin::BlockHash;
use bitcoin::VarInt;
use bitcoin::consensus::Decodable;
use bitcoin::consensus::Encodable;
use bitcoin::hashes::Hash;
use bitcoin::hashes::sha256;
use bitcoin::hex::DisplayHex;
use bitcoin::hex::FromHex;
use floresta_common::read_bounded_len;
use rustreexo::node_hash::BitcoinNodeHash;
use rustreexo::proof::Proof;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Represents a JSON-RPC request (versions 1.0 and 2.0).
pub struct RpcRequest {
    /// The JSON-RPC version, typically "2.0".
    ///
    /// For JSON-RPC 2.0, this field is required. For earlier versions, it may be omitted.
    ///
    /// Source: <`https://json-rpc.dev/docs/reference/version-diff`>
    pub jsonrpc: Option<String>,

    /// The method to be invoked, e.g., "getblock", "sendtransaction".
    pub method: String,

    /// The parameters for the method, json value that must be an array or an object.
    pub params: Option<Value>,

    /// An optional identifier for the request, which can be used to match responses.
    pub id: Value,
}

/// Some utility functions to extract parameters from the request. These
/// methods already handle the case where the parameter is missing or has an
/// unexpected type, returning an error if so.
pub mod arg_parser {

    use serde::Deserialize;
    use serde_json::Value;

    use crate::json_rpc::res::jsonrpc_interface::JsonRpcError;

    /// Extracts a parameter from the request parameters at the specified index.
    ///
    /// This function can extract any type that implements `FromStr`, such as `BlockHash` or
    /// `Txid`. It checks if the parameter exists and is a valid string representation of the type.
    /// Returns an error otherwise.
    pub fn get_at<'de, T: Deserialize<'de>>(
        params: &'de Value,
        index: usize,
        field_name: &str,
    ) -> Result<T, JsonRpcError> {
        if params.is_null() {
            return Err(JsonRpcError::MissingParameter(field_name.to_string()));
        }

        let v = match (params.is_array(), params.is_object()) {
            (true, false) => params.get(index),
            (false, true) => params.get(field_name),
            _ => {
                return Err(JsonRpcError::InvalidParameterStructure(
                    (*params).to_string(),
                ));
            }
        };

        let value = v.ok_or(JsonRpcError::MissingParameter(field_name.to_string()))?;

        T::deserialize(value)
            .map_err(|e| JsonRpcError::InvalidParameterType(format!("{field_name}: {e}")))
    }

    /// Wraps a parameter extraction result so that a missing parameter yields `Ok(None)`
    /// instead of an error. Other errors are propagated unchanged.
    pub fn try_into_optional<T>(
        result: Result<T, JsonRpcError>,
    ) -> Result<Option<T>, JsonRpcError> {
        match result {
            Ok(t) => Ok(Some(t)),
            Err(JsonRpcError::MissingParameter(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Like [`get_at`], but returns `default` when the parameter is missing instead of
    /// an error. Type mismatches are still propagated as errors.
    pub fn get_with_default<'de, T: Deserialize<'de>>(
        v: &'de Value,
        index: usize,
        field_name: &str,
        default: T,
    ) -> Result<T, JsonRpcError> {
        match get_at(v, index, field_name) {
            Ok(t) => Ok(t),
            Err(JsonRpcError::MissingParameter(_)) => Ok(default),
            Err(e) => Err(e),
        }
    }
}

/// The maximum possible inputs you can have per block.
///
/// <https://bitcoin.stackexchange.com/questions/85752/maximum-number-of-inputs-per-transaction>
const MAX_INPUTS_PER_BLOCK: usize = 24_386;

/// How high the Utreexo forest can be.
const MAX_TREE_DEPTH: usize = 64;

/// The maximum number of proof hashes that can be included in a Utreexo proof.
const MAX_PROOF_HASHES: usize = MAX_INPUTS_PER_BLOCK * MAX_TREE_DEPTH;

/// Maximum serialized size of a [`TipProof`] in bytes.
const MAX_PROOF_SIZE_BYTES: usize =
    // block hash
    32
    // targets: varint count + up to MAX_INPUTS_PER_BLOCK varint-encoded targets
    + 9 + MAX_INPUTS_PER_BLOCK * 9
    // proof hashes: varint count + up to MAX_PROOF_HASHES 32-byte hashes
    + 9 + MAX_PROOF_HASHES * 32
    // proven hashes: u32 LE count + up to MAX_INPUTS_PER_BLOCK 32-byte hashes
    + 4 + MAX_INPUTS_PER_BLOCK * 32;

#[derive(Debug, Clone, PartialEq, Eq)]
/// A chain-tip inclusion proof as serialized by utreexod.
///
/// Responses to `proveutxochaintipinclusion` utreexod RPC.
pub struct TipProof {
    /// The block hash at which this proof was generated.
    pub proved_at_hash: BlockHash,

    /// The Utreexo accumulator proof (targets + sibling hashes).
    pub proof: Proof,

    /// The raw leaf hashes that were proven.
    pub hashes_proven: Vec<BitcoinNodeHash>,
}

impl FromStr for TipProof {
    type Err = bitcoin::consensus::encode::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Here we expect for `s` to be a hex string, its len() returns
        // the double of bytes.
        if (s.len() / 2) > MAX_PROOF_SIZE_BYTES {
            return Err(bitcoin::consensus::encode::Error::ParseFailed(
                "Proof exceeds max size allowed",
            ));
        }

        let proof_bytes = Vec::from_hex(s)
            .map_err(|_| bitcoin::consensus::encode::Error::ParseFailed("Invalid hex"))?;

        Self::consensus_decode(&mut proof_bytes.as_slice())
    }
}

impl Decodable for TipProof {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        // Block hash (32 bytes)
        let proved_at_hash = BlockHash::consensus_decode(reader)?;

        // Targets (varint count + varint-encoded)
        let num_targets = read_bounded_len(reader, MAX_INPUTS_PER_BLOCK)?;
        let mut targets = Vec::with_capacity(num_targets);
        for _ in 0..num_targets {
            let target = VarInt::consensus_decode(reader)?;
            targets.push(target.0);
        }

        // Proof sibling hashes (varint count + 32-byte hashes)
        let num_hashes = read_bounded_len(reader, MAX_PROOF_HASHES)?;
        let mut proof_hashes = Vec::with_capacity(num_hashes);
        for _ in 0..num_hashes {
            let hash = sha256::Hash::consensus_decode(reader)?;
            proof_hashes.push(BitcoinNodeHash::Some(hash.to_byte_array()));
        }

        // Hashes proven (u32 LE count + 32-byte hashes)
        let num_proven = u32::consensus_decode(reader)? as usize;
        if num_proven > MAX_INPUTS_PER_BLOCK {
            return Err(bitcoin::consensus::encode::Error::ParseFailed(
                "Too many proven hashes",
            ));
        }
        let mut hashes_proven = Vec::with_capacity(num_proven);
        for _ in 0..num_proven {
            let hash = sha256::Hash::consensus_decode(reader)?;
            hashes_proven.push(BitcoinNodeHash::Some(hash.to_byte_array()));
        }

        Ok(Self {
            proved_at_hash,
            proof: Proof {
                targets,
                hashes: proof_hashes,
            },
            hashes_proven,
        })
    }
}

impl Encodable for TipProof {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, bitcoin::io::Error> {
        let mut len = 0;

        // Block hash (32 bytes)
        len += self.proved_at_hash.consensus_encode(writer)?;

        // Targets (varint count + varint-encoded)
        len += VarInt(self.proof.targets.len() as u64).consensus_encode(writer)?;
        for target in &self.proof.targets {
            len += VarInt(*target).consensus_encode(writer)?;
        }

        // Proof sibling hashes (varint count + 32-byte hashes)
        len += VarInt(self.proof.hashes.len() as u64).consensus_encode(writer)?;
        for hash in &self.proof.hashes {
            len += sha256::Hash::from_byte_array(**hash).consensus_encode(writer)?;
        }

        // Hashes proven (u32 LE count + 32-byte hashes)
        len += (self.hashes_proven.len() as u32).consensus_encode(writer)?;
        for hash in &self.hashes_proven {
            len += sha256::Hash::from_byte_array(**hash).consensus_encode(writer)?;
        }

        Ok(len)
    }
}

impl Serialize for TipProof {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut buf = Vec::new();
        self.consensus_encode(&mut buf)
            .map_err(serde::ser::Error::custom)?;
        serializer.serialize_str(&buf.as_hex().to_string())
    }
}

impl<'de> Deserialize<'de> for TipProof {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let hex_str = String::deserialize(deserializer)?;
        Self::from_str(&hex_str).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tip_proof_tests {
    use bitcoin::consensus::encode::deserialize_hex;

    use super::*;

    /// A valid TipProof with 1 target, 3 proof hashes, and 1 proven leaf hash.
    const TIP_PROOF_HEX: &str = "06db48a6f377f85e46c4e0b915af21c05122c72fff2a8193e19bc68a8b18116d0100030b6c7f2192b1460acf65143badad31b1f000e4940d966bdc8d41463c3049255164028cae25759e119457b059aaa6a38e5812cfd72e9fd07a6393771b4881a2fa80500750ac49f80b161c7431cc3f345a3872147b7adb432f032956eafa9fb76801000000e1e92857db1c66b3cf610e445eb006d68373d82b33398bdadd2f70cf4088dac2";

    /// Just the 32-byte block hash portion of the proof above (no body).
    const TIP_PROOF_BLOCK_HASH_HEX: &str =
        "06db48a6f377f85e46c4e0b915af21c05122c72fff2a8193e19bc68a8b18116d";

    /// Decodes a well-formed proof and checks that every field lands in the right position,
    /// then verifies a serde JSON round-trip produces the same result.
    #[test]
    fn test_tip_proof_valid_deserialization() {
        let proof: TipProof = deserialize_hex(TIP_PROOF_HEX).unwrap();

        assert_eq!(
            proof.proved_at_hash.to_string(),
            "6d11188b8ac69be193812aff2fc72251c021af15b9e0c4465ef877f3a648db06"
        );
        assert_eq!(proof.proof.targets, vec![0]);
        assert_eq!(proof.proof.hashes.len(), 3);
        assert_eq!(proof.hashes_proven.len(), 1);

        // A minimal valid structure with all counts set to zero should also decode.
        let hex = format!("{}000000000000", TIP_PROOF_BLOCK_HASH_HEX);
        let empty_proof: TipProof = deserialize_hex(&hex).unwrap();
        assert!(empty_proof.proof.targets.is_empty());
        assert!(empty_proof.proof.hashes.is_empty());
        assert!(empty_proof.hashes_proven.is_empty());

        // Serde round-trip: serialize to JSON and back, result must be identical.
        let json = serde_json::to_value(&proof).unwrap();
        let deserialized: TipProof = serde_json::from_value(json).unwrap();
        assert_eq!(proof, deserialized);
    }

    /// Exercises various forms of malformed input that the decoder must reject:
    /// trailing bytes, truncated data, oversized counts, and completely empty input.
    #[test]
    fn test_tip_proof_invalid_deserialization() {
        // Extra byte appended after a valid proof
        let trailing = format!("{}ff", TIP_PROOF_HEX);
        assert!(deserialize_hex::<TipProof>(&trailing).is_err());

        // Only the 32-byte block hash, no body at all
        assert!(deserialize_hex::<TipProof>(TIP_PROOF_BLOCK_HASH_HEX).is_err());

        // Varint claiming 24,387 targets (one over MAX_INPUTS_PER_BLOCK)
        let oversized_targets = format!("{}fd435f", TIP_PROOF_BLOCK_HASH_HEX);
        assert!(deserialize_hex::<TipProof>(&oversized_targets).is_err());

        // u32 LE claiming 24,387 proven hashes (one over MAX_INPUTS_PER_BLOCK)
        let oversized_proven = format!("{}0000435f0000", TIP_PROOF_BLOCK_HASH_HEX);
        assert!(deserialize_hex::<TipProof>(&oversized_proven).is_err());

        // Completely empty input
        assert!(deserialize_hex::<TipProof>("").is_err());
    }
}
