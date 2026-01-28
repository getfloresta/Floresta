//! UData is the serialized data used for proof propagation in utreexo. It contains all
//! data needed for validating some piece of information, like a transaction and a block.

use bitcoin::consensus;
use bitcoin::consensus::Decodable;
use bitcoin::consensus::Encodable;
use bitcoin::hashes::sha256;
use bitcoin::hashes::Hash;
use bitcoin::BlockHash;
use bitcoin::OutPoint;
use bitcoin::TxOut;
use sha2::Digest;
use sha2::Sha512_256;

use crate::prelude::Box;
use crate::prelude::Vec;
use crate::pruned_utreexo::consensus::UTREEXO_TAG_V1;

/// Leaf data is the data that is hashed when adding to utreexo state. It contains validation
/// data and some commitments to make it harder to attack an utreexo-only node.
#[derive(Debug, PartialEq)]
pub struct LeafData {
    /// A commitment to the block creating this utxo
    pub block_hash: BlockHash,
    /// The utxo's outpoint
    pub prevout: OutPoint,
    /// Header code is a compact commitment to the block height and whether or not this
    /// transaction is coinbase. It's defined as
    ///
    /// ```ignore
    /// header_code: u32 = if transaction.is_coinbase() {
    ///     (block_height << 1 ) | 1
    /// } else {
    ///     block_height << 1
    /// };
    /// ```
    pub header_code: u32,
    /// The actual utxo
    pub utxo: TxOut,
}

impl LeafData {
    pub fn _get_leaf_hashes(&self) -> sha256::Hash {
        let mut ser_utxo = Vec::new();
        self.utxo
            .consensus_encode(&mut ser_utxo)
            .expect("serializing TxOut never fails: Vec<u8>::Write always returns Ok");

        let leaf_hash = Sha512_256::new()
            .chain_update(UTREEXO_TAG_V1)
            .chain_update(UTREEXO_TAG_V1)
            .chain_update(self.block_hash)
            .chain_update(self.prevout.txid)
            .chain_update(self.prevout.vout.to_le_bytes())
            .chain_update(self.header_code.to_le_bytes())
            .chain_update(ser_utxo)
            .finalize();

        sha256::Hash::from_byte_array(leaf_hash.into())
    }
}

impl Decodable for LeafData {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, consensus::encode::Error> {
        Self::consensus_decode_from_finite_reader(reader)
    }
    fn consensus_decode_from_finite_reader<R: bitcoin::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, consensus::encode::Error> {
        let block_hash = BlockHash::consensus_decode(reader)?;
        let prevout = OutPoint::consensus_decode(reader)?;
        let header_code = u32::consensus_decode(reader)?;
        let utxo = TxOut::consensus_decode(reader)?;
        Ok(LeafData {
            block_hash,
            prevout,
            header_code,
            utxo,
        })
    }
}

/// Commitment of the leaf data, but in a compact way
///
/// The serialized format is:
/// `[<header_code><amount><spk_type>]`
///
/// The serialized header code format is:
///   bit 0 - containing transaction is a coinbase
///   bits 1-x - height of the block that contains the spent txout
///
/// It's calculated with:
///   header_code = <<= 1
///   if IsCoinBase {
///       header_code |= 1 // only set the bit 0 if it's a coinbase.
///   }
/// ScriptPubKeyKind is the output's scriptPubKey, but serialized in a more efficient way
/// to save bandwidth. If the type is recoverable from the scriptSig, don't download the
/// scriptPubKey.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct CompactLeafData {
    /// Header code tells the height of creating for this UTXO and whether it's a coinbase
    pub header_code: u32,
    /// The amount locked in this UTXO
    pub amount: u64,
    /// The type of the locking script for this UTXO
    pub spk_ty: ScriptPubKeyKind,
}

/// A recoverable scriptPubKey type, this avoids copying over data that are already
/// present or can be computed from the transaction itself.
/// An example is a p2pkh, the public key is serialized in the scriptSig, so we can just
/// grab it and hash to obtain the actual scriptPubKey. Since this data is committed in
/// the Utreexo leaf hash, it is still authenticated
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ScriptPubKeyKind {
    /// An non-specified type, in this case the script is just copied over
    Other(Box<[u8]>),
    /// p2pkh
    PubKeyHash,
    /// p2wsh
    WitnessV0PubKeyHash,
    /// p2sh
    ScriptHash,
    /// p2wsh
    WitnessV0ScriptHash,
}

impl Decodable for ScriptPubKeyKind {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, consensus::encode::Error> {
        let ty = u8::consensus_decode(reader)?;
        match ty {
            0x00 => Ok(ScriptPubKeyKind::Other(Box::consensus_decode(reader)?)),
            0x01 => Ok(ScriptPubKeyKind::PubKeyHash),
            0x02 => Ok(ScriptPubKeyKind::WitnessV0PubKeyHash),
            0x03 => Ok(ScriptPubKeyKind::ScriptHash),
            0x04 => Ok(ScriptPubKeyKind::WitnessV0ScriptHash),
            _ => Err(consensus::encode::Error::ParseFailed("Invalid script type")),
        }
    }
}

impl Encodable for ScriptPubKeyKind {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, bitcoin::io::Error> {
        let mut len = 1;

        match self {
            ScriptPubKeyKind::Other(script) => {
                00_u8.consensus_encode(writer)?;
                len += script.consensus_encode(writer)?;
            }
            ScriptPubKeyKind::PubKeyHash => {
                0x01_u8.consensus_encode(writer)?;
            }
            ScriptPubKeyKind::WitnessV0PubKeyHash => {
                0x02_u8.consensus_encode(writer)?;
            }
            ScriptPubKeyKind::ScriptHash => {
                0x03_u8.consensus_encode(writer)?;
            }
            ScriptPubKeyKind::WitnessV0ScriptHash => {
                0x04_u8.consensus_encode(writer)?;
            }
        }
        Ok(len)
    }
}

/// This module provides utility functions for working with Utreexo proofs.
///
/// These functions can be used, for example, when verifying if a mempool transaction is valid;
/// to consume a block (delete transactions included in it from the mempool);
/// or to validate a block.
pub mod proof_util {
    use bitcoin::blockdata::script;
    use bitcoin::blockdata::script::Instruction;
    use bitcoin::consensus::Encodable;
    use bitcoin::hashes::sha256;
    use bitcoin::hashes::Hash;
    use bitcoin::Amount;
    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::OutPoint;
    use bitcoin::PubkeyHash;
    use bitcoin::ScriptBuf;
    use bitcoin::ScriptHash;
    use bitcoin::Transaction;
    use bitcoin::TxIn;
    use bitcoin::TxOut;
    use bitcoin::Txid;
    use bitcoin::WPubkeyHash;
    use bitcoin::WScriptHash;
    use floresta_common::impl_error_from;
    use rustreexo::accumulator::node_hash::BitcoinNodeHash;
    use sha2::Digest;
    use sha2::Sha512_256;

    use super::LeafData;
    use crate::prelude::*;
    use crate::pruned_utreexo::consensus::UTREEXO_TAG_V1;
    use crate::pruned_utreexo::utxo_data::UtxoData;
    use crate::BlockchainError;
    use crate::CompactLeafData;
    use crate::ScriptPubKeyKind;

    #[derive(Debug)]
    /// Errors that may occur while reconstructing a leaf's scriptPubKey.
    pub enum LeafErrorKind {
        /// The witness or scriptsig was empty, so nothing could be inspected.
        EmptyStack,

        /// The scriptsig data could not be parsed into `Instruction`s.
        InvalidInstruction(script::Error),

        /// The last instruction in the scriptsig was not an `OP_PUSHBYTES`.
        NotPushBytes,
    }

    impl fmt::Display for LeafErrorKind {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            match self {
                LeafErrorKind::EmptyStack => write!(f, "empty stack"),
                LeafErrorKind::InvalidInstruction(e) => write!(f, "invalid instruction: {}", e),
                LeafErrorKind::NotPushBytes => write!(f, "expected push bytes instruction"),
            }
        }
    }

    /// Error while reconstructing a leaf's scriptPubKey, returned by `process_proof`.
    ///
    /// This error is triggered if the input lacks the hashed data required by the
    /// [ScriptPubKeyKind] (i.e., the public key for P2PKH, the redeem script for P2SH, or the
    /// witness public key and witness script for P2WPKH/P2WSH).
    #[derive(Debug)]
    pub struct UtreexoLeafError {
        pub leaf: CompactLeafData,
        pub txid: Txid,
        pub vin: usize,
        pub kind: LeafErrorKind,
    }

    impl_error_from!(LeafErrorKind, script::Error, InvalidInstruction);
    impl_error_from!(BlockchainError, UtreexoLeafError, UtreexoLeaf);

    impl Display for UtreexoLeafError {
        fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "failed to reconstruct leaf {:?} for TxIn {}:{}: {:?}",
                self.leaf, self.txid, self.vin, self.kind
            )
        }
    }

    /// This function returns the scriptPubKey type (i.e. address type) of a given script data.
    /// It can be:
    ///
    /// - `PubKeyHash`: A pay-to-public-key-hash script.
    /// - `ScriptHash`: A pay-to-script-hash script.
    /// - `WitnessV0PubKeyHash`: A pay-to-witness-public-key-hash script.
    /// - `WitnessV0ScriptHash`: A pay-to-witness-script-hash script.
    /// - `Other`: a non specified script type. It is just copied over.
    pub fn get_script_type(script: &ScriptBuf) -> ScriptPubKeyKind {
        if script.is_p2pkh() {
            return ScriptPubKeyKind::PubKeyHash;
        }

        if script.is_p2sh() {
            return ScriptPubKeyKind::ScriptHash;
        }

        if script.is_p2wpkh() {
            return ScriptPubKeyKind::WitnessV0PubKeyHash;
        }

        if script.is_p2wsh() {
            return ScriptPubKeyKind::WitnessV0ScriptHash;
        }

        ScriptPubKeyKind::Other(script.to_bytes().into_boxed_slice())
    }

    /// Reconstructs the leaf data from a [CompactLeafData], the UTXO's block hash, and its spending tx input.
    pub fn reconstruct_leaf_data(
        leaf: &CompactLeafData,
        input: &TxIn,
        block_hash: BlockHash,
    ) -> Result<LeafData, LeafErrorKind> {
        let spk = reconstruct_script_pubkey(leaf, input)?;

        Ok(LeafData {
            block_hash,
            header_code: leaf.header_code,
            prevout: input.previous_output,
            utxo: TxOut {
                script_pubkey: spk,
                value: Amount::from_sat(leaf.amount),
            },
        })
    }

    /// Checks if a script is unspendable either by its length or if it contains the `OP_RETURN` opcode.
    /// It follows the implementation on Bitcoin Core.
    fn is_unspendable(script: &ScriptBuf) -> bool {
        if script.len() > 10_000 {
            return true;
        }

        if !script.is_empty() && script.as_bytes()[0] == 0x6a {
            return true;
        }

        false
    }

    /// Computes the hash of a leaf node in the utreexo accumulator.
    #[inline]
    fn get_leaf_hashes(
        txid: Txid,
        is_coinbase: bool,
        vout: u32,
        utxo: &TxOut,
        height: u32,
        block_hash: BlockHash,
    ) -> sha256::Hash {
        // An utreexo leaf hash is computed by hashing the UTXO bytes with some metadata
        let mut ser_utxo = Vec::new();
        utxo.consensus_encode(&mut ser_utxo)
            .expect("serializing TxOut never fails: Vec<u8>::Write always returns Ok");

        // Header code encodes the block height (at the 31 MSB) and coinbase flag (LSB = 1 for coinbase)
        let header_code = if is_coinbase {
            height << 1 | 1
        } else {
            height << 1
        };

        let leaf_hash = Sha512_256::new()
            .chain_update(UTREEXO_TAG_V1)
            .chain_update(UTREEXO_TAG_V1)
            .chain_update(block_hash)
            .chain_update(txid)
            .chain_update(vout.to_le_bytes())
            .chain_update(header_code.to_le_bytes())
            .chain_update(ser_utxo)
            .finalize();

        sha256::Hash::from_byte_array(leaf_hash.into())
    }

    /// From a block, gets the roots that will be included on the acc, certifying
    /// that any utxo will not be spent in the same block.
    pub fn get_block_adds(
        block: &Block,
        height: u32,
        block_hash: BlockHash,
    ) -> Vec<BitcoinNodeHash> {
        // Get inputs from the block, we'll need this HashSet to check if an output is spent
        // in the same block. If it is, we don't need to add it to the accumulator.
        let mut spent = HashSet::new();
        for tx in &block.txdata {
            for input in &tx.input {
                spent.insert((input.previous_output.txid, input.previous_output.vout));
            }
        }

        // Get all leaf hashes that will be added to the accumulator
        let mut adds = Vec::new();
        for tx in &block.txdata {
            let txid = tx.compute_txid();
            let is_cb = tx.is_coinbase();

            for (vout, output) in tx.output.iter().enumerate() {
                let utxo_id = (txid, vout as u32);

                if is_unspendable(&output.script_pubkey) || spent.contains(&utxo_id) {
                    // Do not add unspendable nor already spent utxos
                    continue;
                }
                adds.push(
                    get_leaf_hashes(txid, is_cb, vout as u32, output, height, block_hash).into(),
                );
            }
        }

        adds
    }

    /// A hash map that provides the UTXO data given the outpoint. We will get this data
    /// from either our own cache or the Utreexo proofs, and use it to validate blocks
    /// and transactions.
    type UtxoMap = HashMap<OutPoint, UtxoData>;

    /// A processed proof is a tuple containing a vector of deleted leaf hashes and a UTXO map.
    /// This will be used to process this block
    type ProcessedProof = (Vec<sha256::Hash>, UtxoMap);

    /// This function processes a proof of inclusion for a given block.
    /// It takes in the [`CompactLeafData`] for this block, this block's transactions, the height
    /// and a function to get the block hash for a given height. Then returns a [`Result`] containing
    /// a vector with hashes for deleted leaves, and a `UtxoMap`, which is defined
    /// as [`HashMap<OutPoint, UtxoData>`].
    pub fn process_proof<F, E>(
        leaves: &[CompactLeafData],
        txdata: &[Transaction],
        height: u32,
        get_block_hash: F,
    ) -> Result<ProcessedProof, E>
    where
        F: Fn(u32) -> Result<BlockHash, E>,
        E: From<UtreexoLeafError>,
    {
        // Initialize return values
        let mut del_hashes = Vec::new();
        let mut utxos = HashMap::new();

        let mut leaves_iter = leaves.iter().cloned();

        // Skip coinbase transaction
        for tx in txdata.iter().skip(1) {
            let txid = tx.compute_txid();

            // Collect new UTXOs, which may be spent by later transactions in the block
            for (vout, out) in tx.output.iter().enumerate() {
                utxos.insert(
                    OutPoint::new(txid, vout as u32),
                    UtxoData {
                        txout: out.clone(),
                        is_coinbase: tx.is_coinbase(),
                        creation_height: height,
                        creation_time: 0, // TODO add MTP(`height` - 1)
                    },
                );
            }

            for (vin, input) in tx.input.iter().enumerate() {
                // Only reconstruct UTXOs missing from the map, from prior blocks. Transactions
                // spending uncreated UTXOs yield an invalid deletion hash, failing utreexo verification.
                if utxos.contains_key(&input.previous_output) {
                    continue;
                }
                let leaf = match leaves_iter.next() {
                    Some(leaf) => leaf,
                    None => continue,
                };

                let creation_height = leaf.header_code >> 1;
                // The coinbase flag is the LSB
                let is_coinbase = (leaf.header_code & 1) != 0;

                let hash = get_block_hash(creation_height)?;
                let leaf =
                    reconstruct_leaf_data(&leaf, input, hash).map_err(|e| UtreexoLeafError {
                        leaf,
                        txid,
                        vin,
                        kind: e,
                    })?;

                // Push the UTXO to remove from the set and its leaf hash (deletion hash)
                del_hashes.push(leaf._get_leaf_hashes());
                utxos.insert(
                    leaf.prevout,
                    UtxoData {
                        txout: leaf.utxo,
                        is_coinbase,
                        creation_height,
                        creation_time: 0, // TODO add MTP(`creation_height` - 1)
                    },
                );
            }
        }

        Ok((del_hashes, utxos))
    }

    /// Reconstructs the output script, also called scriptPubKey, from a [CompactLeafData] and
    /// the spending tx input. Returns an error if we can't reconstruct the script (the input
    /// doesn't contain the required data).
    ///
    /// The reconstructed output script is the hash of either a public key or a script (i.e., P2PKH,
    /// P2SH, P2WPKH, and P2WSH).
    ///
    /// The logic behind is:
    ///
    /// For some script types, the output script is just the hash of something that needs to be
    /// revealed at some later stage (e.g., pkh is the hash of a public key that will be revealed
    /// afterwards in the scriptSig, at spend time). Therefore, this information is redundant,
    /// as we have it inside the spending transaction. For types where reconstruction is possible,
    /// we just need to communicate the type with a single byte marker, and the rest can be built
    /// from that using the spending transaction.
    pub fn reconstruct_script_pubkey(
        leaf: &CompactLeafData,
        input: &TxIn,
    ) -> Result<ScriptBuf, LeafErrorKind> {
        match &leaf.spk_ty {
            ScriptPubKeyKind::Other(spk) => Ok(ScriptBuf::from(spk.clone().into_vec())),
            ScriptPubKeyKind::PubKeyHash => {
                let pkhash = get_pk_hash(input)?;
                Ok(ScriptBuf::new_p2pkh(&pkhash))
            }
            ScriptPubKeyKind::WitnessV0PubKeyHash => {
                let pk_hash = get_witness_pk_hash(input)?;
                Ok(ScriptBuf::new_p2wpkh(&pk_hash))
            }
            ScriptPubKeyKind::ScriptHash => {
                let script_hash = get_script_hash(input)?;
                Ok(ScriptBuf::new_p2sh(&script_hash))
            }
            ScriptPubKeyKind::WitnessV0ScriptHash => {
                let witness_program_hash = get_witness_script_hash(input)?;
                Ok(ScriptBuf::new_p2wsh(&witness_program_hash))
            }
        }
    }

    /// Computes the public key hash from the pushed key in the input's scriptSig.
    fn get_pk_hash(input: &TxIn) -> Result<PubkeyHash, LeafErrorKind> {
        match input.script_sig.instructions().last() {
            None => Err(LeafErrorKind::EmptyStack),
            // Only valid if it's a push bytes instruction
            Some(Ok(Instruction::PushBytes(b))) => Ok(PubkeyHash::hash(b.as_bytes())),
            Some(Ok(_)) => Err(LeafErrorKind::NotPushBytes),
            Some(Err(e)) => Err(e.into()),
        }
    }

    /// Computes the script hash from the input's scriptSig.
    fn get_script_hash(input: &TxIn) -> Result<ScriptHash, LeafErrorKind> {
        match input.script_sig.instructions().last() {
            None => Err(LeafErrorKind::EmptyStack),
            // Only valid if it's a push bytes instruction
            Some(Ok(Instruction::PushBytes(b))) => Ok(ScriptHash::hash(b.as_bytes())),
            Some(Ok(_)) => Err(LeafErrorKind::NotPushBytes),
            Some(Err(e)) => Err(e.into()),
        }
    }

    /// Computes the witness public key hash from the input's witness data.
    fn get_witness_pk_hash(input: &TxIn) -> Result<WPubkeyHash, LeafErrorKind> {
        match input.witness.last() {
            Some(pk) => Ok(WPubkeyHash::hash(pk)),
            None => Err(LeafErrorKind::EmptyStack),
        }
    }

    /// Computes the witness script hash from the input's witness data.
    fn get_witness_script_hash(input: &TxIn) -> Result<WScriptHash, LeafErrorKind> {
        match input.witness.last() {
            Some(script) => Ok(WScriptHash::hash(script)),
            None => Err(LeafErrorKind::EmptyStack),
        }
    }
}

#[cfg(test)]
mod test {
    extern crate std;

    use std::str::FromStr;

    use bitcoin::blockdata::script;
    use bitcoin::consensus::encode::deserialize_hex;
    use bitcoin::opcodes::all::OP_NOP;
    use bitcoin::opcodes::all::OP_PUSHBYTES_1;
    use bitcoin::Amount;
    use bitcoin::BlockHash;
    use bitcoin::ScriptBuf;
    use bitcoin::Transaction;
    use bitcoin::TxIn;
    use floresta_common::bhash;

    use super::proof_util::reconstruct_leaf_data;
    use super::CompactLeafData;
    use super::LeafData;
    use super::ScriptPubKeyKind;
    use crate::proof_util::reconstruct_script_pubkey;
    use crate::proof_util::LeafErrorKind;

    macro_rules! assert_recover_spk {
        (
            $tx_hex:literal,
            $height:literal,
            $index:literal,
            $amount:literal,
            $block_hash:literal,
            $spk_type:ident,
            $expected_spk:literal
        ) => {
            let s: Transaction = deserialize_hex($tx_hex).unwrap();
            let leaf = CompactLeafData {
                amount: Amount::from_btc($amount).unwrap().to_sat(),
                header_code: $height,
                spk_ty: ScriptPubKeyKind::$spk_type,
            };
            let spk =
                super::proof_util::reconstruct_leaf_data(&leaf, &s.input[0], bhash!($block_hash))
                    .unwrap();
            assert_eq!(
                spk.utxo.script_pubkey,
                ScriptBuf::from_hex($expected_spk).unwrap()
            )
        };
    }

    macro_rules! assert_recover_spk_err {
        ($spk_kind:ident, $tx_in:expr, $err_kind:ident $( ( $inner:pat ) )?) => {
            let compact = CompactLeafData {
                header_code: 0,
                amount: 1,
                // The only relevant field to return the different error kinds
                spk_ty: ScriptPubKeyKind::$spk_kind,
            };
            let err = reconstruct_script_pubkey(&compact, &$tx_in).unwrap_err();

            assert!(
                matches!(err, LeafErrorKind::$err_kind $( ( $inner ) )?),
                "Expected LeafErrorKind::{}, got {err:?}",
                stringify!($err_kind),
            );
        };
    }

    #[test]
    fn test_spk_recovery() {
        // p2pkh
        assert_recover_spk!(
            "010000000114baa734ec1a75e84726af2da3abcd41fe9d96f3f8b7e99bcefdfc040cffc2ba030000006a47304402202f89e2deb17f0c2c5732d6f7791a2731703cb128dc86ae0bf288e55a3d7ce9d6022051c2242ca0885a4a2054391385eda03132616fb0c2daa61d6823eff7a21b5d0c01210395c223fbf96e49e5b9e06a236ca7ef95b10bf18c074bd91a5942fc40360d0b68fdffffff04b400000000000000536a4c5058325bc5b3f7d4e7acf388d63ab92d14d7f8f8bcdff384bddd4668f283df0bfe1c2f7728ec1e550ca841794fa28e16e154c5b92b5a1d1d98db4e89f15726bf75e352fe000bddf10068000bd72600012bc20000000000000017a914352481ec2fecfde0c5cdc635a383c4ac27b9f71e87c20000000000000017a9144890aae025c84cb72a9730b49ca12595d6f6088d8772aa0900000000001976a914bf2646b8ba8b4a143220528bde9c306dac44a01c88ac00000000",
            0,
            777548,
            0.03956721,
            "000000000000000000066caa76847c109010eb58402d7a5bf05cc201a011071d",
            PubKeyHash,
            "76a914bf2646b8ba8b4a143220528bde9c306dac44a01c88ac"
        );
        // p2sh
        assert_recover_spk!(
            "0200000001ff1ba24eb11f1290b293b2c5520e4863ffedcc4a4ed9e4933334639ecbcc946500000000fc00473044022001460e6d06dc44e163ef1f692d275a1e357d086d0361fbe5012dbf18cbf2617202207f9e8fb54e776d7e98a6425da2be15e2ffca2e623b7617234226eafe77c70eaa01473044022076d756a250ad4044e2b4a0049112d87367b2f0ce80253e400f3ba09d620cbbdd022020f67b65f7cb5e109b8ccbc852e30b4e84b0b682136a5e72f679bd581b271ea8014c695221021c04b91bffe90c3e4defd021a4b6da4983b97e13c772bf15009f1661480658832102be11f7f0d9696ef731c13ed8b6e955df43cd4238d694a1698b02fcb3c2d275322102e0ad7274a4e93b3b30793ff7a04a31d2792ed22a563fe5ea0095af844c10c9c453aefdffffff02fd5403000000000017a914351bb17d072fff46336baec11a6a8d13ab6b590e87305837000000000017a9147b8d77369df3d2172b0d56792308d7f2635ca79087f1dd0b00",
            0,
            777548,
            0.03956721,
            "00000000000000000005a784e2b5006b34ff63644408df00bfc1a0b1b9507021",
            ScriptHash,
            "a914ed9371b30de550c0617cd0c4b2c0c0dc5e88c65487"
        );
        //p2wpkh
        assert_recover_spk!(
            "01000000000101a742910d02da84259631288eab229ca2bdd39ed7edc8811ca125dc0bcf2b654c0100000000ffffffff02ba150a000000000016001406a9852b7c9f4ff9993b5d2192ac42a5df54828e34c812000000000016001486bdf86c7cbce4841f95b4d8ef101ce8a306e6ad0247304402202936300c12249c8696bb90addcc9482995429d7be0418260178ddc0c630c10ed02206128cac337841b171d15d9aadc2af77d280da7cd85c049149c8134ddb5adc8a10121038adb3497e025c0ff14521a789af4f10d526ec4c95348e708ebdc4d5ac58228e500000000",
            1,
            777716,
            0.01893642,
            "00000000000000000002264d1e3f90280ededd1587c7af932480dac3e2480048",
            WitnessV0PubKeyHash,
            "001406a9852b7c9f4ff9993b5d2192ac42a5df54828e"
        );
        //p2wsh
        assert_recover_spk!(
            "01000000000101cacacdfdc79620cac8bc463cdac9864f557fdb73b6ef0dea8e0d74297d2e4c1a0100000000ffffffff0280841e000000000017a914cef5ab6252860ada719556abebe952c79c466f86878af74e0c00000000220020701a8d401c84fb13e6baf169d59684e17abd9fa216c8cc5b9fc63d622ff8c58d04004730440220289b2e0b6aec5a8f43d283edef0757206de77e3f3acdb322ade452a0468764db02201c332ec46a2ed3614fe392c4011063f39e77def57d89991ccbb99b6c7de2491901473044022044eaf71bdb4b3f0b0ba2f1eec82cad412729a1a4d5fc3b2fa251fecb73c56c0502201579c9e13b4d7595f9c6036a612828eac4796902c248131a7f25a117a0c68ca8016952210375e00eb72e29da82b89367947f29ef34afb75e8654f6ea368e0acdfd92976b7c2103a1b26313f430c4b15bb1fdce663207659d8cac749a0e53d70eff01874496feff2103c96d495bfdd5ba4145e3e046fee45e84a8a48ad05bd8dbb395c011a32cf9f88053ae00000000",
            0,
            487740,
            2.08551770,
            "0000000000000000004fce5d650f72e8f288e8c81b36377c3c7de3d2bc5b3118",
            WitnessV0ScriptHash,
            "0020701a8d401c84fb13e6baf169d59684e17abd9fa216c8cc5b9fc63d622ff8c58d"
        );
        assert_recover_spk!(
            "020000000001018f97e04dd76eec325c149ad417175f01f71b45523d8df79d2745cfee110eabf20000000000ffffffff015cddc71d000000002200204ae81572f06e1b88fd5ced7a1a000945432e83e1551e6f721ee9c00b8cc3326001015100000000",
            0,
            27366,
            4.99637721,
            "000000069585e4b2517a8862d527558ff18df7d4b8c2795b249c116aba9c6c98",
            WitnessV0ScriptHash,
            "00204ae81572f06e1b88fd5ced7a1a000945432e83e1551e6f721ee9c00b8cc33260"
        );
    }

    #[test]
    fn test_invalid_spk_recovery() {
        fn dummy_txin(script_sig: ScriptBuf) -> TxIn {
            TxIn {
                script_sig,
                ..Default::default()
            }
        }

        let empty_txin = dummy_txin(ScriptBuf::new());
        assert_recover_spk_err!(PubKeyHash, empty_txin, EmptyStack);
        assert_recover_spk_err!(ScriptHash, empty_txin, EmptyStack);
        assert_recover_spk_err!(WitnessV0PubKeyHash, empty_txin, EmptyStack);
        assert_recover_spk_err!(WitnessV0ScriptHash, empty_txin, EmptyStack);

        let mut script_sig = ScriptBuf::new();
        for _ in 0..255 {
            // Trying with different lengths of the script_sig
            script_sig.push_opcode(OP_NOP);

            let non_push_txin = dummy_txin(script_sig.clone());
            assert_recover_spk_err!(PubKeyHash, non_push_txin, NotPushBytes);
            assert_recover_spk_err!(ScriptHash, non_push_txin, NotPushBytes);
            assert_recover_spk_err!(WitnessV0PubKeyHash, non_push_txin, EmptyStack);
            assert_recover_spk_err!(WitnessV0ScriptHash, non_push_txin, EmptyStack);
        }
        // Using an `OP_PUSHBYTES` without data, i.e., an invalid instruction
        script_sig.push_opcode(OP_PUSHBYTES_1);

        let invalid_txin = dummy_txin(script_sig);
        assert_recover_spk_err!(
            PubKeyHash,
            invalid_txin,
            InvalidInstruction(script::Error::EarlyEndOfScript)
        );
        assert_recover_spk_err!(
            ScriptHash,
            invalid_txin,
            InvalidInstruction(script::Error::EarlyEndOfScript)
        );
        assert_recover_spk_err!(WitnessV0PubKeyHash, invalid_txin, EmptyStack);
        assert_recover_spk_err!(WitnessV0ScriptHash, invalid_txin, EmptyStack);
    }

    #[test]
    fn test_reconstruct_leaf_data() {
        let leaf: LeafData = deserialize_hex("f99e24b9e96a3c6220449b2bf520d6a9562237e2f4fc6f6b2ba57a71de000000e6f50efb6747f836ca3510df3da120fdb2ae4cf62893cc014e08c25dab70248b01000000cc000400b429653b4f0600001600142b91c8f80b071c5f60e1a512d49a6a544e51165b").unwrap();
        let spending_tx: Transaction = deserialize_hex("02000000000101e6f50efb6747f836ca3510df3da120fdb2ae4cf62893cc014e08c25dab70248b0100000000feffffff02dbe6553b4f0600001600148d57f8da7fc15371dc14d35e97850ab564a17b1240420f0000000000220020ed59bf193c5197a5b1dbbbc723ddeca82cdfbb188218b3ede50150e1890fc55202473044022024979ec4bda473b71288b2c15390418d7d300551aa5e463cc6b64acd5c3070b50220444c94242aff2ba1bd966308d60f537524b0755931d545d98e1fc45239ff6b08012103de7c420624c009d6a5761871e78b39ff864887f885ed313e27f778b3772e74916a000200").unwrap();

        let compact = CompactLeafData {
            amount: Amount::from_btc(69373.68668596).unwrap().to_sat(),
            header_code: 262348,
            spk_ty: ScriptPubKeyKind::WitnessV0PubKeyHash,
        };
        let reconstructed = reconstruct_leaf_data(
            &compact,
            &spending_tx.input[0],
            bhash!("000000de717aa52b6b6ffcf4e2372256a9d620f52b9b4420623c6ae9b9249ef9"),
        )
        .unwrap();
        assert_eq!(leaf, reconstructed);
    }
}
