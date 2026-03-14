// SPDX-License-Identifier: MIT OR Apache-2.0

use bitcoin::consensus::Decodable;
use bitcoin::consensus::Encodable;
use bitcoin::hashes::sha256d;
use bitcoin::hashes::Hash;
use bitcoin::hashes::HashEngine;
use bitcoin::Block;
use bitcoin::Txid;
use floresta_common::prelude::*;
use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleProof {
    pub target: Txid,
    pub pos: u64,
    pub hashes: Vec<sha256d::Hash>,
}

impl Default for MerkleProof {
    fn default() -> Self {
        Self::new()
    }
}

impl MerkleProof {
    /// Creates an empty proof
    fn new() -> Self {
        MerkleProof {
            target: Txid::all_zeros(),
            hashes: Vec::new(),
            pos: 0,
        }
    }

    /// Return the hashes for this proof as a [`Vec<String>`]
    pub fn to_string_array(&self) -> Vec<String> {
        self.hashes().iter().map(|hash| hash.to_string()).collect()
    }

    /// Returns the hashes for this proof
    pub fn hashes(&self) -> Vec<sha256d::Hash> {
        self.hashes.clone()
    }

    /// Creates a new proof from a list of hashes and a target. Target is a 64 bits
    /// unsigned integer indicating the index of a transaction we with to prove. Note that
    /// this only proves one tx at the time.
    pub fn from_block_hashes(tx_list: Vec<sha256d::Hash>, target: u64) -> Self {
        let target_hash = tx_list[target as usize];
        let (_, proof) = Self::transverse(tx_list, Vec::new(), target);
        Self {
            target: target_hash.into(),
            pos: target,
            hashes: proof,
        }
    }

    /// Same as [MerkleProof::from_block_hashes] but you give a block instead of a list of
    /// hashes.
    pub fn from_block(block: &Block, target: u64) -> Self {
        let tx_list: Vec<_> = block
            .txdata
            .iter()
            .map(|tx| tx.compute_txid().to_raw_hash())
            .collect();
        Self::from_block_hashes(tx_list, target)
    }

    /// Verifies a proof by hashing up all nodes until reach a root, and compare `root` with
    /// computed root.
    pub fn verify(&self, root: sha256d::Hash) -> Result<bool, String> {
        let mut computed = self.target.to_raw_hash();
        let mut placement = self.pos;
        for hash in self.hashes.iter() {
            if placement & 1 == 0 {
                computed = Self::parent_hash(computed.as_ref(), hash.as_ref());
            } else {
                computed = Self::parent_hash(hash.as_ref(), computed.as_ref());
            }

            placement >>= 1;
        }
        Ok(root == computed)
    }

    /// Returns the position of a node's parent
    fn get_parent(pos: u64) -> u64 {
        (pos ^ 1) / 2
    }

    /// Returns a node's sibling. This is useful because we have to copy a node's sibling
    /// to proof, so we can compute it's parent.
    fn get_sibling(pos: u64) -> u64 {
        pos ^ 1
    }

    /// Computes the hash of two node's parent, by taking sha256d(left_child | right_child), where |
    /// means byte-wise concatenation.
    fn parent_hash(left: &[u8], right: &[u8]) -> sha256d::Hash {
        let mut engine = sha256d::Hash::engine();
        engine.input(left);
        engine.input(right);
        sha256d::Hash::from_engine(engine)
    }

    /// Iterates over the tree, collecting required nodes for proof, internally we compute
    /// all intermediate nodes, but don't keep them.
    fn transverse(
        nodes: Vec<sha256d::Hash>,
        mut proof: Vec<sha256d::Hash>,
        target: u64,
    ) -> (Vec<sha256d::Hash>, Vec<sha256d::Hash>) {
        // We reached a root. This is the recursion base
        if nodes.len() == 1 {
            return (nodes, proof);
        }
        // Here we store all nodes for the next row
        let mut new_nodes = Vec::new();
        // Grab a node's sibling. In a Merkle Tree, our target nodes are given, and its parent
        // can be computed using available data. We must only provide a node's sibling, so verifier
        // can get a parent hash.
        let sibling = Self::get_sibling(target);

        // This if catches an edge case where we try to get a sibling from the last node
        // in a non-perfect tree. This yields an out-of-bound read from nodes.
        if sibling != nodes.len() as u64 {
            proof.push(nodes[sibling as usize]);
        } else {
            proof.push(nodes[target as usize]);
        }
        // If the row has a odd number of nodes, we must repeat the last node to force it
        // even.
        let node_count = nodes.len();

        let pairs = if node_count % 2 == 0 {
            node_count / 2
        } else {
            // (node_count + 1) / 2
            node_count.div_ceil(2)
        };

        for idx in 0..pairs {
            if (2 * idx + 1) >= node_count {
                new_nodes.push(Self::parent_hash(
                    nodes[2 * idx].as_ref(),
                    nodes[2 * idx].as_ref(),
                ));
            } else {
                new_nodes.push(Self::parent_hash(
                    nodes[2 * idx].as_ref(),
                    nodes[2 * idx + 1].as_ref(),
                ));
            }
        }
        Self::transverse(new_nodes, proof, Self::get_parent(target))
    }
}

impl Decodable for MerkleProof {
    fn consensus_decode<R: bitcoin::io::Read + ?Sized>(
        reader: &mut R,
    ) -> Result<Self, bitcoin::consensus::encode::Error> {
        let pos = u64::consensus_decode(reader)?;
        let target = Txid::consensus_decode(reader)?;
        let len = u64::consensus_decode(reader)?;
        let mut hashes = Vec::new();
        for _ in 0..len {
            let hash = sha256d::Hash::consensus_decode(reader)?;
            hashes.push(hash);
        }
        Ok(Self {
            hashes,
            pos,
            target,
        })
    }
}

impl Encodable for MerkleProof {
    fn consensus_encode<W: bitcoin::io::Write + ?Sized>(
        &self,
        writer: &mut W,
    ) -> bitcoin::io::Result<usize> {
        let mut len = 0;
        len += self.pos.consensus_encode(writer)?;
        len += self.target.consensus_encode(writer)?;

        let hashes_len = self.hashes.len() as u64;
        len += hashes_len.consensus_encode(writer)?;

        for hash in self.hashes.iter() {
            len += hash.consensus_encode(writer)?;
        }

        Ok(len)
    }
}

#[cfg(test)]
mod test {
    use core::str::FromStr;

    use bitcoin::consensus::deserialize;
    use bitcoin::hashes::hex::FromHex;
    use bitcoin::hashes::sha256d;
    use floresta_common::prelude::*;

    use super::MerkleProof;
    #[test]
    fn test_merkle_root() {
        let hashes = Vec::from([
            "9fe0683d05e5a8ce867712f0f744a1e9893365307d433ab3b8f65dfc59d561de",
            "9e2804f04a9d52ad4b67e10cba631934915a7d6d083126b338dda680522bb602",
            "01ad659d8d3f17e96d54e4240614fad5813a58cc1ac67a336839b0bf6c56f2d3",
            "8627dad7e4df3cc60d1349aac61cae36436423429a12f3df9a1e54a5ca8ee008",
            "5f82784d819f440ee1766d9802d113c54626bd613009cbf699213f49adf2fbbd",
        ]);
        let root = sha256d::Hash::from_str(
            "ff8fa20a8da05e334d59d257c8ba6f76b31856fafe92afdb51151daa2fe0a240",
        )
        .unwrap();
        let hashes: Vec<_> = hashes
            .iter()
            .map(|txid| sha256d::Hash::from_str(txid).unwrap())
            .collect();
        let proof = MerkleProof::from_block_hashes(hashes, 2);
        assert_eq!(Ok(true), proof.verify(root));
    }
    #[test]
    fn test_serialization() {
        use bitcoin::consensus::serialize;
        let hashes = Vec::from([
            "9fe0683d05e5a8ce867712f0f744a1e9893365307d433ab3b8f65dfc59d561de",
            "9e2804f04a9d52ad4b67e10cba631934915a7d6d083126b338dda680522bb602",
            "01ad659d8d3f17e96d54e4240614fad5813a58cc1ac67a336839b0bf6c56f2d3",
            "8627dad7e4df3cc60d1349aac61cae36436423429a12f3df9a1e54a5ca8ee008",
            "5f82784d819f440ee1766d9802d113c54626bd613009cbf699213f49adf2fbbd",
        ]);
        let root = sha256d::Hash::from_str(
            "ff8fa20a8da05e334d59d257c8ba6f76b31856fafe92afdb51151daa2fe0a240",
        )
        .unwrap();
        let hashes: Vec<_> = hashes
            .iter()
            .map(|txid| sha256d::Hash::from_str(txid).unwrap())
            .collect();

        let proof = MerkleProof::from_block_hashes(hashes, 2);
        let ser_proof = serialize(&proof);
        let de_proof = deserialize::<MerkleProof>(&ser_proof).unwrap();

        assert_eq!(de_proof, proof);
        assert!(de_proof.verify(root).unwrap());
    }
    #[test]
    fn test_from_block() {
        // Example from signet block 114448. This has a edge case of the target transaction
        // being the last one in a odd number of elements, this caused this code to break in the past
        // credit to @jaonoctus for finding it.
        let block_hex = Vec::from_hex("000000200e7a1b4acac9d0fede38780af685f4f2468f379441da88d9333190e9fd000000de17ded487dedf4febfc2062696f726daf82c387c40ac6bf3f730cb6b8078ea6c7cb5c631e52011eafd2850109010000000001010000000000000000000000000000000000000000000000000000000000000000ffffffff040310bf01feffffff02cdf7052a0100000016001481113cad52683679a83e76f76f84a4cfe36f75010000000000000000776a24aa21a9ed1a57f94172436261a598a05fd00f5dc6f0af113118b2ad0fd5ab067cdf14d0844c4fecc7daa2490047304402200188f50f763b594ad2515b5fe7a6ccd0651cb21a57f7c9462517809b0a71056a022073de9d51712ed92333fdf103021ab15f9d7ee438a9f40c4d21a2aae25e30a746010001200000000000000000000000000000000000000000000000000000000000000000000000000200000000010600f477f4573600d59279b8590ad2f393e80e35a4747a6bb980ed435cae516a470100000000fdffffffeeed54083b3a3b3d5fbd2b89dbd584cf9473caecff5307ef6a45d4bf895d3c9f0100000000fdffffff71998948a594919d5f458ca629851af2cddc6a707a46953946fa169785809cc50000000000fdffffff95aff1490be1d5cd7c0147cad2f09e85419661322abe55177e6c37cf74a274da0100000000fdffffff3989e6239f74d9f01863b9cdeb139c911f864e56f42d9a423e2fb56b8acff0e40100000000fdffffffa14095a3c7134dcfe63670990f1aae604e9c75c995bbe4e786d5dad9e210bdfa0100000000fdffffff011e21730200000000160014167905ff5769be1088fba282d5f2fe083eafd24f024730440220524dae54f383a34a3605a0fdd403f5e69e991675d4d631925fd38b16f11d965b02207946cd9a39407d722cb3407913dfd13970b347473e87fdf2dfa3d5ea9dc2e14d012102e62ff0a4b5f94bed13bda49c6f28016a4c462246ca234d91010050d67a766d850247304402206e2410b4b76d09b4c67c84f005d84e46e9d444495734dd3037f1cd8ac938de8602206da11b400e7b02a7ee6a51cb7b42b893db214a2ffff2eac75502429f50b03f8301210215a12de3be0588cc75b0f4c313dae06dbd9ecee933532f4d792bc25fa7a866aa024730440220255e7e199d8dfdc3764e7328b96a26d014f337d27f6629022bc4d6499af832020220703ae4dee6b14a568c877e31af8ead209316891a33eee085dc695ae7fc1cfc84012103dceb3c814a400f39c67d4cd71f926bf1e1dc8944445f712a3ea583ea5f4d1e9b0247304402203a5eb82548dd0ff5f443b66e7f26f5c846315cfbfe488854027dc417c27993090220058ce815c913b0a4b24bd8d9db0a350f5c3bb2b4dcdec2a002a24c440b8496e0012102e8f9662c11aec882442f42dad5d7c19373249793a2a6b630c1971c8e2930ad0f0247304402203411bb65d910def1892d83dba89c6ea6664313eda18af70248884efe4ec6c204022029845c30b2904b60ab5fe6e19cf448f970dfe61cf2e4079482c55f1bb8b8cf4a0121030e8756107674bf33e2392d77f20d2890570c605282e665ec56358b72861219da02473044022075517e3dcfde63549abb19db17fe8a88b2cc643c921b295388c79f78936d3d8802202699e190c42a53856e99211f7b2a702fbc29466bfcac5d7a10e57fd77c109b1801210381477a1e64fda3873c3833c7cbe12cf0a8379a10c57e143be887cf5a53fe46bc0ebf0100020000000001011963991b6c03ca15f9ec0e4ad611e474c3277d0c3655bc9d797de1c14ab7aa7c0100000000feffffff0240420f00000000001600142349b57a01d75c7c858ac751f897239a86bbf04bb873285d5106000016001414bf9b0fe92caa0097b63a1ef3ea275d410fd98d02473044022054b442f21f988a0b97e6d7e8b24e23ccfecfaa12c0eb9eb96775a7765af941de022050d04a2db06e86fe9c9f90fee1169648cf9531e4c3697a744296f6b61324f4830121028bf1e30de43373796a0991b98aa5a4195c86495fba684656a26b0096df3110990fbf010002000000000101344f1ef28a664fe17a4aa50b8d3009ef6c6e49b85d66f8f9b3449bb74582ea350000000000feffffff0240420f0000000000160014e44b51eb316763098c5d18de6ca8b2c0a59c0e17bc9ce6df4f0600001600145fd025abef9e808689076765f4f0c56738c59f39024730440220479115072916500cffc1cc52fd889e5bf832a353125a1bdece97f7563feb5e090220504c4c7dd6a1f6cb0c2ba50c44187f46075dd8b20f94ad6c8b6bafc790dbcc31012102f2bdad8ec4652cb21e7605f26bc81ca66c10ff28e0f3d2a0ea19fc8cd52099b10fbf010002000000000101edd9a6cc38a24a387dc9abe7a1701886fcb2320657b144d0d0316e8ca43ae3d70000000000feffffff0240420f000000000022512010e8ae98031a5708b4bf6569c51f2fff6b000d0237d094bba411b684bb91357aafcb93035206000016001460f37086e9aaae4ddbf4b239482368a10d9c41c70247304402201ee3fdce8f0fb88e23ebdb43fab44044d099f85f506a93feb18b3dd2396b10f002206e82dab913ea09a62509b34b9d1799682af3dc2852a7ca3a851f0d377f8f387001210263bb044157db46c69b30e280e1b012aeeef7e4d2e324bb1badf6b271eea081d50fbf010002000000000101716c42179f6c5c18bb8c648fbb5697e4ada7a3661d6dc41988898b4a3366f00b0000000000feffffff028fbb1b0c000000001600145cb211255a52a72c50ff28125e6d19c3783634246a2b010000000000160014b323d1a6d231f481ec9d0edb4b6628ad7c3013ed02473044022013a5cf7f8b40b66b49d1d3b0da3b045c55eb65da1f0ab5b51925ad1ab74676420220670a2cc57fdd95bec289f3659254683401f550ead6c518ea69b2ac5e154a984d012102638e70a1e4a118a68d0a222d6783bb89308e54c403b44347c140c47d9f3f00290fbf0100020000000001018aece6fc6208c756036d569293b3f309fd753b7e72aef0d57d9d937db72721cf0100000000feffffff02590db89c5006000016001456e2f3da4da33aef14ddfcb6b732a81ff7e8675d40420f00000000002251205e15d348f226d1ea063a79e25aace48990dc473f61ae23f517e07f2917ac721c024730440220545d47834d82f7701cf229d5261f7de206f935ba4b0b58bb89d515add9a900d402207fd29ec10e8e2c935aba34c22d9dc8fc37b32cb0ab4ab4bff91fe6c68456b0c1012103ae5be7da80c63939f16a801363033d52c4e551b55eb1329d09ffa244e0d793290fbf010002000000000101911cf9966a3d8cd2bfb90f6f5d099c1e568a55bdcc55c4e6c041fc600614a3520100000000feffffff020dce63d44e0600001600145cee73b449f3da151659deb159148753c283381140420f0000000000225120b3b15df11fed9bca782f500031c94efc4ed8e4b0521c00295784d7dc5ac8684c02473044022046abbd3a4c279ddd535c3695bb89300f46494411f29c617862ccbb38b923ee740220638ad662b069c9977d4ac51065b493545ff3f4a890c05b42717be6d5ab9b3bd201210254f0f69224472b7f6a0849587a171637916a274d7ddef895dba298e6a1d89dbfadbe0100020000000001013d154c4a311dc630d1b3b403a327f03c07348b6c6cf0557500ef518ca412a2990000000000feffffff026d0833374d060000160014359a35960efb7c6ad62119702edf555163c03ab640420f0000000000225120dd0440d706666db07ae597a02d7af497d8fe1fdf059d1ca0b3c13b6d00a7d24f02473044022038535e6b04102ac866e896a33692e4050339de1f078b375c72d14914133cf9c6022034ad2f08092667cf8e5c60973771bef68315a47dff73f8ad4f0ef1d4a7d863110121032a42df4c108ad44282a75746f52161759aff3adc38bde305be7f5ddf172860120ebf0100").unwrap();
        let block = deserialize(&block_hex).unwrap();
        let merkle = MerkleProof::from_block(&block, 8);

        assert!(merkle
            .verify(*block.header.merkle_root.as_raw_hash())
            .unwrap());
    }
}
