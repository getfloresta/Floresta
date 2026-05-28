// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use std::io::Cursor;

use arbitrary::Arbitrary;
use arbitrary::Unstructured;
use bitcoin::BlockHash;
use bitcoin::CompactTarget;
use bitcoin::TxMerkleNode;
use bitcoin::block::Version;
use bitcoin::blockdata::block::Header as BlockHeader;
use bitcoin::consensus::Decodable;
use bitcoin::consensus::Encodable;
use bitcoin::hashes::Hash;
use floresta_chain::DiskBlockHeader;
use floresta_chain::InvalidReason;
use libfuzzer_sys::fuzz_target;

fn gen_header(u: &mut Unstructured<'_>) -> arbitrary::Result<BlockHeader> {
    // Pull primitives from the fuzzer:
    let version = i32::arbitrary(u)?;
    let prev: [u8; 32] = Arbitrary::arbitrary(u)?;
    let merkle: [u8; 32] = Arbitrary::arbitrary(u)?;
    let time = u32::arbitrary(u)?;
    let bits = u32::arbitrary(u)?;
    let nonce = u32::arbitrary(u)?;

    Ok(BlockHeader {
        version: Version::from_consensus(version),
        prev_blockhash: BlockHash::from_byte_array(prev),
        merkle_root: TxMerkleNode::from_byte_array(merkle),
        time,
        bits: CompactTarget::from_consensus(bits),
        nonce,
    })
}

#[derive(Arbitrary)]
struct Inputs {
    #[arbitrary(with = gen_header)]
    header: BlockHeader,
    height: u32,
    tag: u8,
    reason_tag: u8,
}

fuzz_target!(|data: &[u8]| {
    if let Ok(inp) = Inputs::arbitrary(&mut Unstructured::new(data)) {
        let header = match inp.tag % 6 {
            0 => DiskBlockHeader::FullyValid(inp.header, inp.height),
            1 => DiskBlockHeader::AssumedValid(inp.header, inp.height),
            2 => DiskBlockHeader::HeadersOnly(inp.header, inp.height),
            3 => DiskBlockHeader::InFork(inp.header, inp.height),
            4 => DiskBlockHeader::Orphan(inp.header),
            _ => {
                let reason = match inp.reason_tag % 3 {
                    0 => InvalidReason::ValidationFailed,
                    1 => InvalidReason::Invalidated,
                    _ => InvalidReason::DescendsFromInvalid,
                };
                DiskBlockHeader::InvalidChain(inp.header, inp.height, reason)
            }
        };

        // Encode
        let mut buf = Vec::new();
        let written = header.consensus_encode(&mut buf).expect("encode failed");
        assert_eq!(written, buf.len(), "encode returned wrong length");

        // Decode and compare
        let decoded =
            DiskBlockHeader::consensus_decode(&mut Cursor::new(&buf)).expect("decode failed");
        assert_eq!(decoded, header, "roundtrip mismatch");
    }
});
