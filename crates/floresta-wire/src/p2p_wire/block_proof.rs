//! This module defines the structures and methods for handling Utreexo proofs in the context of
//! messaging in the bitcoin network.
//!
//! Utreexo is a dynamic accumulator, used to represent the UTXO set in a succinct way. This
//! allows for lightweight clients that still perform full-validation. The trade-off is that
//! for every UTXO being spent, you need a proof showing that UTXO is part of the UTXO set.
//!
//! This proof can be downloaded from the network, as long as you are connected to at least one
//! utreexo archive node. You first download the actual block. Then you figure out which inputs you
//! need -- you may not need all of them, due to some of them being cached by your local client.
//! Then you request them by sending a [GetUtreexoProof] message.
//!
//! This message asks for one block hash, whether to include leaf data (the data associated with
//! a given UTXO), and two bitmaps: one for the proof hashes, and one for the leaf indices. These
//! bitmaps are used to indicate which inputs you want to include in the proof. You list the
//! inputs that were not created in the same block, in the same order they appear in block. If you
//! need it, you set the associated bit to `true`, otherwise you set it to `false`.
//!
//! You'll then receive a [UtreexoProof] message, which contains the block hash, the proof hashes,
//! and the leaf data for UTXOs being spent. You can then use this data to validate the block, and
//! update your local Utreexo forest.

use bitcoin::consensus::Decodable;
use bitcoin::consensus::Encodable;
use bitcoin::hashes::sha256;
use bitcoin::BlockHash;
use bitcoin::VarInt;
use floresta_chain::CompactLeafData;
use floresta_chain::ScriptPubKeyKind;
use floresta_common::read_bounded_len;
use rustreexo::accumulator::node_hash::BitcoinNodeHash;

/// The maximum possible inputs you can have per block.
///
/// The smallest block (header + coinbase) you can have is 145B instead of 146B as stated in the
/// stackexchange answer, but that doesn't change the max possible input value.
///
/// <https://bitcoin.stackexchange.com/questions/85752/maximum-number-of-inputs-per-transaction>
pub const MAX_INPUTS_PER_BLOCK: usize = 24_386;

/// How high the Utreexo forest can be.
const MAX_TREE_DEPTH: usize = 64;

/// The maximum number of proof hashes that can be included in a Utreexo proof.
///
/// Assuming that each UTXO needs a proof, with no overlaps of any kind, the maximum number of
/// proof hashes is the number of inputs per block multiplied by the maximum number of
/// elements that each proof requires, in a tree with `MAX_TREE_DEPTH` depth.
pub const MAX_PROOF_HASHES: usize = MAX_INPUTS_PER_BLOCK * MAX_TREE_DEPTH;

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A Bitmap used to request proof elements in Utreexo proofs.
///
/// This bitmap represents which inputs the sender needs in the proof.
/// Each bit in the bitmap corresponds to an input in the block, where `true` means
/// the input should be included in the proof.
pub struct Bitmap {
    /// The actual bytes representing the bitmap.
    bytes: Vec<u8>,

    /// How many elements have been pushed into the bitmap.
    n_inputs: u32,
}

impl Bitmap {
    /// Creates a new empty Bitmap.
    ///
    /// Use the `push_input` method to add inputs to the bitmap.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a new input to the bitmap.
    /// This method takes a boolean `request` which indicates whether the input
    /// should be requested from our remote peer.
    pub fn push_input(&mut self, request: bool) {
        let bit_offset = self.n_inputs % u8::BITS;
        self.n_inputs += 1;

        // First bit of a new byte: push the byte (0 or 1)
        if bit_offset == 0 {
            self.bytes.push(request as u8);
            return;
        }

        // Otherwise, if needed, set the bit in the already-present last byte
        if request {
            *self.bytes.last_mut().expect("byte was pushed") |= 1u8 << bit_offset;
        }
    }

    /// Returns whether the bitmap is empty (i.e., has no inputs).
    pub fn is_empty(&self) -> bool {
        self.n_inputs == 0
    }
}

impl Encodable for Bitmap {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, bitcoin::io::Error> {
        let mut len = 0;
        len += self.bytes.consensus_encode(writer)?;
        Ok(len)
    }
}

/// Represents a Utreexo proof request, for a specific block.
pub struct GetUtreexoProof {
    /// The block hash for which the proof is requested.
    pub block_hash: BlockHash,

    /// Whether to include leaf data in the proof.
    pub include_leaves: bool,

    /// A bitmap indicating which proof hashes to include in the response.
    pub proof_hashes_bitmap: Bitmap,

    /// A bitmap indicating which leaf indices to include in the response.
    pub leaf_index_bitmap: Bitmap,
}

impl Encodable for GetUtreexoProof {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> Result<usize, bitcoin::io::Error> {
        let mut len = 0;
        len += self.block_hash.consensus_encode(writer)?;
        len += self.include_leaves.consensus_encode(writer)?;
        len += self.proof_hashes_bitmap.consensus_encode(writer)?;
        len += self.leaf_index_bitmap.consensus_encode(writer)?;

        Ok(len)
    }
}

#[derive(Debug, Clone)]
/// Represents a Utreexo proof for a specific block.
///
/// This message will be sent in response to a [GetUtreexoProof] request.
pub struct UtreexoProof {
    /// The block hash for which the proof is provided.
    pub block_hash: BlockHash,

    /// The proof hashes for the requested inputs.
    pub proof_hashes: Vec<BitcoinNodeHash>,

    /// The targets for the requested inputs.
    ///
    /// Targets are numerical values representing the positions of the UTXOs being spent inside the
    /// forest. Using the target positions and hashes, we can recompute the forest roots, and
    /// verify whether the proof is valid.
    pub targets: Vec<u64>,

    /// The leaf data for the requested inputs.
    pub leaf_data: Vec<CompactLeafData>,
}

impl Decodable for UtreexoProof {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let block_hash = BlockHash::consensus_decode(reader)?;

        // Read the proof hashes
        let n_hashes = read_bounded_len(reader, MAX_PROOF_HASHES)?;

        let mut proof_hashes = Vec::with_capacity(n_hashes);
        for _ in 0..n_hashes {
            let hash = sha256::Hash::consensus_decode(reader)?;
            proof_hashes.push(hash.into());
        }

        // Read the targets
        let n_targets = read_bounded_len(reader, MAX_INPUTS_PER_BLOCK)?;
        let mut targets = Vec::with_capacity(n_targets);
        for _ in 0..n_targets {
            let target = VarInt::consensus_decode(reader)?;
            targets.push(target.0);
        }

        // Read the leaf data
        let n_leaf_data = read_bounded_len(reader, MAX_INPUTS_PER_BLOCK)?;
        let mut leaf_data = Vec::with_capacity(n_leaf_data);
        for _ in 0..n_leaf_data {
            let leaf = CompactLeafData {
                header_code: u32::consensus_decode(reader)?,
                amount: u64::consensus_decode(reader)?,
                spk_ty: ScriptPubKeyKind::consensus_decode(reader)?,
            };

            leaf_data.push(leaf);
        }

        Ok(UtreexoProof {
            block_hash,
            proof_hashes,
            targets,
            leaf_data,
        })
    }
}

#[cfg(test)]
mod utreexo_proof_tests {
    use std::str::FromStr;

    use bitcoin::consensus::encode::deserialize_hex;
    use bitcoin::hashes::sha256;
    use bitcoin::Block;
    use bitcoin::BlockHash;
    use bitcoin::Network;
    use bitcoin::Transaction;
    use floresta_chain::proof_util::process_proof;
    use floresta_chain::AssumeValidArg;
    use floresta_chain::BlockchainError;
    use floresta_chain::ChainState;
    use floresta_chain::FlatChainStore;
    use floresta_chain::FlatChainStoreConfig;
    use floresta_common::acchashes;
    use floresta_common::bhash;
    use rustreexo::accumulator::node_hash::BitcoinNodeHash;
    use rustreexo::accumulator::proof::Proof;
    use rustreexo::accumulator::stump::Stump;

    use crate::block_proof::UtreexoProof;
    use crate::p2p_wire::block_proof::Bitmap;

    const PROOF_DATA: &str = "0000000000000274e38b79d9c971de45f66d80bd2a586efa3e947ab448c65f0a21db2d9d4e21d41ec389811a282bddd70d2822bbc31d5c36bd25f2f7268bd2660ef15151f2df93ade1fa38c3807f6cb7446c5eb78131f188a5b5f4049b6b337090bf472b504976cbe726b549ca6fdd5aa198e0340eef4cc54609dd2b1b0e287d85379f13885795902a87a71e2c118dcf92e7b8d63d6ee2db727d1d535082817b5bf7b687299b0f8bedba623915bc8c11f37bbb852ee132ed28f1662dacd237babe4c0a5a306f1e0415e2db8e14d260c02f4b55f79552aff452bef1d66024c1ec07c365744675c11701c8b79cb942b42f5b120597dbe0ab2da7d60d502d799de9a91d11fb7546e30eb3e402bcca609436f3bb324808e0c143064799bd33691d892f60722b6bbacc53f9ceedf13dc0d88d93f7b407745537b0121fbb02e77b875995375b3b3cffc1c8b7101b45bfe963f91a5916bc46f755cfe578b21c2fa65d8caccbb69854d0246e0166f677ca4faeb7320523dc78b1d3c52d5a27700ba1f6061a6ca5297daa71ee816589e733bd455da6677aed14f0dde90c083dac1da7284e3fb72d97eb35051d774aa71fe3d434e28c227dbd53c1ae166ae76ac6fbaa744641d4a1050efbbc3c752f0a22f4187f600b8fe6be363597512ed6b5425ce017057b155edc7b9d3d04c52ab698cec15e32eeeb87e9b7bbc49ef6e1f25fd2400d2dda47c7239006477bcb15d5c5adb17e36b8df6ee0e26c8c778cdd21e8b3bd4970e3cc89d33d3ee3e6f16c00249e2a562f0f99a33fa12ae0130a67c137619d58c04a0e50e464bced58301ab8c0992b26653a15c0b2866e4af60d1172013d2d84c017c07cf7352220e579bb7989d938f34ac88cd73b45bf5e653346fd163380a5db1aa5537f1d2fafa09311dbf3c1478e16e21caed7eaebfab03c55cd7968f099d64a36411a4d7e2d23d7fa99ad8a33d701480ecfd426772d1961a0038af07bc3fca40d5ee4c9a212f5e1c437466d1f00875a6d42d04edfdafefd71046b96e1df309571bda1cbe97e8dc8b5efa0581a48e5d8fbe26066c01571b7dd6de32bc06d827bd78a03a80cff2b53374cc6fa61efff77aeda1e602a256b72f328187abb359bf10b60a36d599e18bbd71c98ee5dd910ba5efdad88ee45de41bd7fdb1775fcadcd60f9bc4e144f040e667cea94049d7f1df31cb54594da5879d027783bde41d4b658a2c540e41a2c04ff5b4169a2ee5492fe6a17e27700fb85bb717f4c5c955acb12f52bce5b9b7c90656cf9bbda51b0e80c82c9b4ff7967b1c0c8c5908d6c1c71e23b75268a6ada55750ff0a6e0d2c6ec3e29051aa5e632ad1bdc2c457fde45de454ca8e544b3c57da06acad7e3fff8aa0e23227fb9e40f192591f7d015d770e620187e065b47e8c5909d26a63a2da2047b26755c3970719c06285f7baa81e4071019ec78ca45522155090f11d42bff420034095608bc466cebcd51ba00514bcee815e587430248a07f9da39b0f3b2f2fafa5f667c59a9a03d1a7e2c7463433130afe2d024900fe46024900fe4a024900fe48024900fe4c024900fe4e024900fe50024900febd014900fee8024900fee60249000a04040500aeaa232a01000000010404050067dc751e0100000001040405000a002916010000000104040500f2415b05010000000104040500104ffaeb000000000104040500c53c13ea00000000010404050000dd5ce80000000001fc0305001beac82700000000010804050010a81442000000000108040500b0affb640000000001";

    const BLOCK: &str = "0100000011d60326c600e89bcea01a9884670bff6ba841728e70cd959d04000000000000a358f735ae382a260b709e818d55d5074f4bf0a930293a58e27baf19f18e9d203d19254f3fd40c1a62aa10e50b01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff34043e19254f010a2c4d4d3d3d8eb7a0c4ff07bdb0eb81f5f85f5c6865a9abc1552cfd4119191682bb5d9007910100000000000000ffffffff01a078072a0100000023210278aac0dccc3046120e0076a4cbfef47d4ff1d407f9597ffc21062ab55be3b2f7ac0000000001000000015f2cf7f37d5a83fc9ad514e42d415ef0342574712ecc365f55ca7c0ba79e12ba000000008b483045022100ef215fbea3427afaefccfe6159e63d303e2d658b46adf3de0e5e2c11f5590f2e022074bc3f5ae713290db79dfcef41ab669d92558a2b2f7f64c81e83f14ca887b841014104a39b9e4fbd213ef24bb9be69de4a118dd0644082e47c01fd9159d38637b83fbcdc115a5d6e970586a012d1cfe3e3a8b1a3d04e763bdc5a071c0e827c0bd834a5ffffffff02aee8371e010000001976a9140568015a9facccfd09d70d409b6fc1a5546cecc688ac00c2eb0b000000001976a91400591cd5fe5cfcefb38fa3d5ca7b1667ec52ac3388ac000000000100000001d71be7b3b3ea37c8ee8bbdc588366a6555e4cbeeca5173cd0d634a109716f630000000008a47304402200cba67f2c1e806ae2e5ee5a893cc55f549cf0af530cd86e532873b94552b5144022059249e1f3ca460f4d0661f3b3f00c59699e1fd4b50ec5396caaa4133aa27004e014104a39b9e4fbd213ef24bb9be69de4a118dd0644082e47c01fd9159d38637b83fbcdc115a5d6e970586a012d1cfe3e3a8b1a3d04e763bdc5a071c0e827c0bd834a5ffffffff0267cafb1d010000001976a9140568015a9facccfd09d70d409b6fc1a5546cecc688ac00127a00000000001976a914987c9643069837ea82cdfc6287fc74e7766fde2f88ac0000000001000000019574d909d8de1697acf4674d7031d28adb28b834c1b295ba80911a593f9003ad000000008b483045022023e6d08ec35dd9b7f77fb5eb6806647fb7bf2c46b056ccfa57314ccbe64eff0602210093d134951d33d139ebd20df4e6ce1a9b756cf49913f858415e965644b9e4afd6014104a39b9e4fbd213ef24bb9be69de4a118dd0644082e47c01fd9159d38637b83fbcdc115a5d6e970586a012d1cfe3e3a8b1a3d04e763bdc5a071c0e827c0bd834a5ffffffff020adc3415010000001976a9140568015a9facccfd09d70d409b6fc1a5546cecc688ac0024f400000000001976a914c5d64c5c00d62d80a69ae7d63dc081edc71dbc8e88ac0000000001000000011b1389c7ec6011d1a7f80d097607273bc0eb4fca4090936cfd13c4187469e6d3000000008b48304502204f4119c864e8a0e646b83740979db2f330e2b64243f2da7d85b931762ee0a673022100d7f909522d6c55dc899773d5c810dfea3837012b008876fd9a4094441f8a7b54014104a39b9e4fbd213ef24bb9be69de4a118dd0644082e47c01fd9159d38637b83fbcdc115a5d6e970586a012d1cfe3e3a8b1a3d04e763bdc5a071c0e827c0bd834a5ffffffff02f26065ff000000001976a9140568015a9facccfd09d70d409b6fc1a5546cecc688ac00e1f505000000001976a914f54b60efd3e70eb2f4866e7bd8db698bb13aa59788ac000000000100000001d3f9fc5aad3884b447e5921fe747e5aa6c1ef5bc354dfa96c5154e9d8aad40fb000000008a473044022002f329d215d6ea3311003ae5a996122f25e404864df4792f7094b34e823fdc37022000d573a1d114ebf43428e79280c853ceb46aa390fb075459d59454a755d3174d014104a39b9e4fbd213ef24bb9be69de4a118dd0644082e47c01fd9159d38637b83fbcdc115a5d6e970586a012d1cfe3e3a8b1a3d04e763bdc5a071c0e827c0bd834a5ffffffff02d00cebeb000000001976a9140568015a9facccfd09d70d409b6fc1a5546cecc688ac40420f00000000001976a91412257a70afb08e512cb9f91479a1edb9de6b81fe88ac000000000100000001507ee46589e2a81df64e2d5ee008c27e6a4e928fa95b7648da9328c1e9f91a9c000000008b483045022100e50513d07dca027abe419e9ef4eed989e40b4ea0a70e06db0aa3545752379419022011aa017ed5de218379ac6dd087563c086b7d4468a39dbb2b3128283e7117751d014104a39b9e4fbd213ef24bb9be69de4a118dd0644082e47c01fd9159d38637b83fbcdc115a5d6e970586a012d1cfe3e3a8b1a3d04e763bdc5a071c0e827c0bd834a5ffffffff02c5b57fe6000000001976a9140568015a9facccfd09d70d409b6fc1a5546cecc688ac00879303000000001976a914212441eebd4f77e2e3ef8c2d1bf0ff1687b3c87a88ac000000000100000001052a4d7756177801ace95de9b4d90d1f4bc7dfdb757baad72696453aa26085f8000000008a47304402207d692726dde4ef6bc88dd91e42d2d0df3098bff898fe739ab0599225d63c5a91022029224f844ca00e43f091c951a913012b7fbbad51619d5724149a7cf0dbe62b97014104a39b9e4fbd213ef24bb9be69de4a118dd0644082e47c01fd9159d38637b83fbcdc115a5d6e970586a012d1cfe3e3a8b1a3d04e763bdc5a071c0e827c0bd834a5ffffffff0240dc64b0000000001976a9140568015a9facccfd09d70d409b6fc1a5546cecc688acc000f837000000001976a914968845256e95c1982a9e60f060ea39f35792f0b788ac000000000100000001fb4cbcbd2436c632f73db5272608acb8dee23733d05f10b3f376f0c0d7b75cbd000000008b48304502210092b1b11f12176a708291b608c259eaff9ce3e98fb9db66175da25374860668f7022048f55939440743f9925b3d34232e613715718d10119a5c23c71f46cdacb49f63014104a39b9e4fbd213ef24bb9be69de4a118dd0644082e47c01fd9159d38637b83fbcdc115a5d6e970586a012d1cfe3e3a8b1a3d04e763bdc5a071c0e827c0bd834a5ffffffff0293816e27000000001976a9140568015a9facccfd09d70d409b6fc1a5546cecc688ac88685a00000000001976a9140d238c7c0ce909d511c193efcccb804584f6dbf888ac0000000001000000013afec4b3b670276e281bf3b6626ced8bc8356faee44348cbb4c65096af7dabfd000000008b483045022100e1cfbe6cdb54d081614fe2675d8554fdbd9271d616d875df30b046dd55d991be022013b9b63e78b260300a32219a2631f6b86d68492415c0f1b228010b195c6d3e53014104e1412429a76076dc25b2a7fc98c93da7b6e3e8045d48fecc3cb67c9163f6d616392458bdbf1e25c50171883619f91992dfe0abbecaac307e23be8e26ce9d19f8ffffffff0380a20442000000001976a914cea0534850d52cc6f19dadaf93ce30db9302187c88ac20a10700000000001976a9142911fad6636d73faea7ada74a30032c32620a94f88ac20a10700000000001976a914618118641b48c1679a3cc7ad00a9d246aa695fd888ac0000000001000000018fb56eb2ccdd91d7eb42e3e47515c828470ab88e721463fcafe9e7122e143f65000000008a47304402203fcdb0b731282e5a142305a304d23737a84ae8c7ce4bdd54d477846b572939d502205fbf74660a1a6f3eaaa48e0c5d24d9178f94c12498a269056d970f190a00197f0141040fed4775d5f5b47f2d73b35974b26ae19ccbde2c5a2ce741e5172c0baaa32bd30c23c664a5b91d84cf6c59e62a1c217dc318e24ce42977cbd8eaa89046d57639ffffffff0f005a6202000000001976a914dfcc1e75593def6563d3e35fa6615be5fd4f560e88ac80f0fa02000000001976a914ba23c86fe7c519d30f743e3491a67ef5d27c447388ac00e1f505000000001976a914dab269b027f6432ae7a6c34930485a4c8c9f5ace88ac0065cd1d000000001976a91408645206ee8a59c8d876e8bbb1a9512f5b1b14ea88ac00e1f505000000001976a914f16e0dbc76ccf3018f381b7726ce83cf874e444e88ac00e1f505000000001976a9146d293ad798d4cba8532b4f604e5c585674593d3b88ac00e1f505000000001976a91427be1119f8de54a8a7d3e65f131c6ca42eed7ba888ac80969800000000001976a914075c1dd720c63e10588b7a158aa7f43d4a6dd56288ac80969800000000001976a914c7d4c137526e35257fb94af3bd862fd8f1b65b5f88ac00e1f505000000001976a91495913e7275bfb35b3a0a9b749a013d56e54791f788ac80969800000000001976a914539845befa9b3017e0e42bfc6d1806b1d04dea2488ac00e1f505000000001976a9140a25839f64951ab152d0983d88d32cdc59120f7b88ace0151718000000001976a914ae4568bb485b0859c0d6db04d2094f3e5c43d2ce88ac80969800000000001976a9148c5a2a5c19f3f492a7df212c41f5e6df3504f4e988ac00879303000000001976a91425570998b4dfd2eb7c892c6d61772b0d209b050588ac00000000";

    #[test]
    fn test_build_bitmap() {
        let mut bitmap = Bitmap::new();

        bitmap.push_input(false); //              0
        bitmap.push_input(true); //              10
        bitmap.push_input(false); //            010
        bitmap.push_input(false); //           0010
        bitmap.push_input(true); //           10010
        bitmap.push_input(true); //          110010
        bitmap.push_input(false); //        0110010
        bitmap.push_input(false); //       00110010
        assert_eq!(bitmap.bytes, vec![0b00110010]);

        bitmap.push_input(false); //     0 00110010
        assert_eq!(bitmap.bytes, vec![0b00110010, 0]);

        bitmap.push_input(false); //    00 00110010
        bitmap.push_input(true); //    100 00110010

        let final_bitmap = vec![0b00110010, 0b100];
        assert_eq!(bitmap.n_inputs, 11);
        assert_eq!(bitmap.bytes, final_bitmap);
    }

    #[test]
    fn test_empty_bitmap_serialization() {
        use bitcoin::consensus::encode::serialize;

        let bitmap = Bitmap::new();
        let serialized = serialize(&bitmap);
        assert_eq!(serialized, vec![0x00]);
    }

    fn to_acc_hashes(vec: Vec<sha256::Hash>) -> Vec<BitcoinNodeHash> {
        vec.into_iter().map(Into::into).collect()
    }

    #[test]
    fn test_process_proof_utxo_order() {
        fn setup_test_chain(
            network: Network,
            assume_valid: AssumeValidArg,
        ) -> ChainState<FlatChainStore> {
            let datadir = format!("./tmp-db/{}.sync_node", rand::random::<u32>());
            let config = FlatChainStoreConfig::new(datadir);
            let store = FlatChainStore::new(config).expect("Failed to create chain store");
            ChainState::new(store, network, assume_valid)
        }

        // STEP 0: Set up the block and utreexo data
        let block: Block = deserialize_hex(BLOCK).unwrap();
        let proof: UtreexoProof = deserialize_hex(PROOF_DATA).unwrap();

        let height = 164_357;

        // This block spends UTXOs from three previous blocks
        let get_block_hash = |height| match height {
            164_354 => Ok(bhash!(
                "0000000000000274e38b79d9c971de45f66d80bd2a586efa3e947ab448c65f0a"
            )),
            164_350 => Ok(bhash!(
                "0000000000000305e4bb508c0b92a288ccac79b4c06bb92220a12650589e79df"
            )),
            164_356 => Ok(bhash!(
                "000000000000049d95cd708e7241a86bff0b6784981aa0ce9be800c62603d611"
            )),
            _ => Err(BlockchainError::BlockNotPresent),
        };

        let state = setup_test_chain(Network::Bitcoin, AssumeValidArg::Disabled);
        let acc = Stump {
            leaves: 4784881,
            roots: acchashes![
                "ad9b9fc757b185e729219b88c01011ddc57f7c009c16d52b45f18e03e4e4c8b8",
                "dc2a0fd968da4098822fb9941e7cd448336a6f8c4f0c3d5f170f6ea7323e125d",
                "6b6020ee6bb92cdd976b099de8e5ad86dd82a002b046899414006409149ddcd3",
                "158861d1078a9f394313afacaf70dd653d6439fcab287964cb1ff15ee0a4e232",
                "89000eaf560a5ba3a15af0dd3cf06aab929a9478060e0d4eda6e215f81ea82c7",
                "fae7b8f7dc1c8d2476002baa278b0381eebf88359c2da052228e3ea9110a67b4",
                "346b5f1bede34d73d9cdf3a78d29a35785a903abc397a1ce614a41158bc0bf7d",
                "b6b56f1ccc990e380ce8c528bb20d92919653d2c12d6d794aa7bac84ca9458e0",
                "7cad5f60ff32a8939d803d289c4d15cc44e98a2db2aa47e79bcf6712ea745ba6",
            ]
            .to_vec(),
        };

        // STEP 1: Verify the accumulator and the block
        let (del_hashes, utxos) =
            process_proof(&proof.leaf_data, &block.txdata, height, get_block_hash).unwrap();
        let r_proof = Proof {
            targets: proof.targets,
            hashes: proof.proof_hashes,
        };

        if !acc.verify(&r_proof, &to_acc_hashes(del_hashes)).unwrap() {
            panic!("Proof must be valid")
        }

        state
            .validate_block_no_acc(&block, height, utxos)
            .expect("Block validation must pass for the given UTXOs map");

        // STEP 2: Add a tx that tries to spend an UTXO created later in the block; utreexo fails
        let spending_tx: Transaction = deserialize_hex("0100000001ed5598cd2a9d8c4782e553dfc149a4a14402d1e4cfe61ae104f8cdf116486341000000008b483045022100aa2d2e5647b2ee15e42effd358946ce7d9515e5a56c21c9f6661b8d3f6f3fc51022028e433bcd457cb40bfdc253d6448f3bc2fc5bb7cc417cbf2458a9803489b9be5014104a39b9e4fbd213ef24bb9be69de4a118dd0644082e47c01fd9159d38637b83fbcdc115a5d6e970586a012d1cfe3e3a8b1a3d04e763bdc5a071c0e827c0bd834a5ffffffff0213ebd526000000001976a9140568015a9facccfd09d70d409b6fc1a5546cecc688ac80969800000000001976a91449bab0d204e6c1fdf99b0b8e0ebf70ab6421e2f488ac00000000").unwrap();
        assert_eq!(
            format!("{}", spending_tx.compute_txid()),
            "d55a5cb093e7d354ebb4ae3ecc12f281f17ca1194ff1d3ab16dd97cfd7eacf46",
        );

        let mut invalid_txdata = block.txdata.clone();
        invalid_txdata.insert(1, spending_tx);

        let (del_hashes, _utxos) =
            process_proof(&proof.leaf_data, &invalid_txdata, height, get_block_hash).unwrap();

        if acc.verify(&r_proof, &to_acc_hashes(del_hashes)).unwrap() {
            panic!("Proof must be invalid")
        }
    }
}
