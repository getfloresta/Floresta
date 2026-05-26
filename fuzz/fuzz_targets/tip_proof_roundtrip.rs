// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::io::Cursor;

use arbitrary::Arbitrary;
use arbitrary::Unstructured;
use bitcoin::BlockHash;
use bitcoin::consensus::Decodable;
use bitcoin::consensus::Encodable;
use bitcoin::hashes::Hash;
use floresta_wire::block_proof::TipProof;
use libfuzzer_sys::fuzz_target;
use rustreexo::node_hash::BitcoinNodeHash;
use rustreexo::proof::Proof;

fn gen_blockhash(u: &mut Unstructured<'_>) -> arbitrary::Result<BlockHash> {
    let bytes: [u8; 32] = Arbitrary::arbitrary(u)?;
    Ok(BlockHash::from_byte_array(bytes))
}

const MAX_TARGETS: usize = 64;
const MAX_HASHES: usize = 64;

fn gen_proof(u: &mut Unstructured<'_>) -> arbitrary::Result<Proof> {
    let n_targets = u8::arbitrary(u)? as usize % (MAX_TARGETS + 1);
    let mut targets = Vec::with_capacity(n_targets);
    for _ in 0..n_targets {
        targets.push(u64::arbitrary(u)?);
    }

    let n_hashes = u8::arbitrary(u)? as usize % (MAX_HASHES + 1);
    let mut hashes = Vec::with_capacity(n_hashes);
    for _ in 0..n_hashes {
        let bytes: [u8; 32] = Arbitrary::arbitrary(u)?;
        hashes.push(BitcoinNodeHash::Some(bytes));
    }

    Ok(Proof { targets, hashes })
}

fn gen_hashes_proven(u: &mut Unstructured<'_>) -> arbitrary::Result<Vec<BitcoinNodeHash>> {
    let n = u8::arbitrary(u)? as usize % (MAX_HASHES + 1);
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        let bytes: [u8; 32] = Arbitrary::arbitrary(u)?;
        v.push(BitcoinNodeHash::Some(bytes));
    }
    Ok(v)
}

#[derive(Arbitrary)]
struct Inputs {
    #[arbitrary(with = gen_blockhash)]
    proved_at_hash: BlockHash,
    #[arbitrary(with = gen_proof)]
    proof: Proof,
    #[arbitrary(with = gen_hashes_proven)]
    hashes_proven: Vec<BitcoinNodeHash>,
}

fuzz_target!(|data: &[u8]| {
    if let Ok(inp) = Inputs::arbitrary(&mut Unstructured::new(data)) {
        let tip_proof = TipProof {
            proved_at_hash: inp.proved_at_hash,
            proof: inp.proof,
            hashes_proven: inp.hashes_proven,
        };

        // Encode
        let mut buf = Vec::new();
        let written = tip_proof.consensus_encode(&mut buf).expect("encode failed");
        assert_eq!(written, buf.len(), "encode returned wrong length");

        // Decode and compare
        let decoded = TipProof::consensus_decode(&mut Cursor::new(&buf)).expect("decode failed");
        assert_eq!(decoded, tip_proof, "roundtrip mismatch");
    }
});
