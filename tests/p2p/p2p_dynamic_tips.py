# SPDX-License-Identifier: MIT OR Apache-2.0

"""
p2p_dynamic_tips.py

Verify that dynamically derived chain tips survive a florestad restart.

After IBD completes, fork headers are injected via a P2P peer. The test
confirms the fork headers are accepted (retrievable via getblockheader),
then restarts florestad and verifies they are still available — proving
that tips are correctly re-derived from the fork file on disk.
"""

import time
from io import BytesIO

import pytest
from test_framework.messages import CBlock, CBlockHeader, msg_headers

# Regtest nBits (difficulty 1 — nearly any nonce satisfies PoW)
REGTEST_NBITS = 0x207FFFFF


def header_from_raw_hex(hex_str):
    """Deserialize a raw 80-byte block header hex string into a CBlockHeader."""
    header = CBlockHeader()
    header.deserialize(BytesIO(bytes.fromhex(hex_str)))
    return header


def create_fork_header(parent_header):
    """Create a valid regtest fork header building on the given parent.

    Uses a different merkle root than the real chain so the block hash
    differs from any existing block at the same height, while still
    satisfying the PoW target.
    """
    block = CBlock()
    block.hashPrevBlock = parent_header.hash_int
    block.nTime = parent_header.nTime + 1
    block.nBits = REGTEST_NBITS
    block.hashMerkleRoot = 0xDEADBEEF
    block.nNonce = 0
    block.solve()
    return CBlockHeader(block)


def assert_fork_headers_present(rpc, fork_hashes, context):
    """Assert all fork hashes are retrievable via getblockheader."""
    for fh in fork_hashes:
        assert (
            rpc.get_blockheader(fh, False) is not None
        ), f"Fork header {fh} not found {context}"


@pytest.mark.p2p
def test_dynamic_chain_tips_derivation_restart(
    setup_logging,
    node_manager,
    florestad_bitcoind_utreexod_with_chain,
):
    """
    Mine a main chain, inject fork headers via P2P, verify they are accepted,
    restart florestad, and verify the fork headers are still retrievable
    (re-derived from the fork file on disk).
    """
    log = setup_logging

    # 1. Mine main chain, sync florestad through IBD
    log.info("Mining 100 blocks and syncing nodes")
    florestad_node, bitcoind_node, utreexod_node = (
        florestad_bitcoind_utreexod_with_chain(floresta_descriptors=[])
    )
    node_manager.wait_for_sync_nodes(is_finished_ibd=True)
    log.info("Florestad IBD complete")

    # 2. Collect parent headers from bitcoind at heights 1..5
    log.info("Collecting parent headers for fork points")
    parent_headers = [
        header_from_raw_hex(
            bitcoind_node.rpc.get_blockheader(bitcoind_node.rpc.get_blockhash(h), False)
        )
        for h in range(1, 6)
    ]

    # 3. Create 5 independent fork headers via CBlock.solve()
    log.info("Creating 5 fork headers")
    fork_headers = [create_fork_header(p) for p in parent_headers]
    fork_hashes = [fh.hash_hex for fh in fork_headers]

    # 4. Disconnect peers, connect P2P peer, send fork headers
    log.info("Disconnecting peers and connecting P2P peer")
    florestad_node.rpc.disconnectnode(node_address=bitcoind_node.p2p_url)
    florestad_node.rpc.disconnectnode(node_address=utreexod_node.p2p_url)
    node_manager.wait_for_peers_connections(
        florestad_node, bitcoind_node, is_connected=False
    )
    node_manager.wait_for_peers_connections(
        florestad_node, utreexod_node, is_connected=False
    )

    peer = node_manager.add_p2p_connection_default(
        node=florestad_node, p2p_idx=0, supports_v2_p2p=False
    )

    log.info("Sending fork headers via P2P")
    for header in fork_headers:
        peer.send_without_ping(msg_headers([header]))
        time.sleep(0.1)
    peer.sync_with_ping()
    time.sleep(2)

    # 5. Verify forks accepted: getblockheader succeeds for each fork hash
    log.info("Verifying fork headers are accepted")
    best_hash = florestad_node.rpc.get_bestblockhash()
    assert_fork_headers_present(florestad_node.rpc, fork_hashes, "before restart")
    for fh in fork_hashes:
        assert fh != best_hash, "Fork header should not be the best block"

    # 6. Restart florestad
    log.info("Restarting florestad")
    florestad_node.stop()
    florestad_node.start()
    florestad_node.rpc.wait_on_socket(opened=True)

    # 7. Verify forks survived restart (re-derived from fork file)
    log.info("Verifying fork headers survived restart")
    assert (
        florestad_node.rpc.get_bestblockhash() == best_hash
    ), "Best block hash changed after restart"
    assert_fork_headers_present(florestad_node.rpc, fork_hashes, "after restart")

    log.info("All fork headers survived restart — tips re-derived correctly")
