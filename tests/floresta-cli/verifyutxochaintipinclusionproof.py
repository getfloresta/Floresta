# SPDX-License-Identifier: MIT OR Apache-2.0

"""
verifyutxochaintipinclusionproof.py

Functional tests for the `verifyutxochaintipinclusionproof` RPC command,
exercising every code branch:
  - valid proof with both verbosity levels (0 and 1)
  - proof verified against an explicit blockhash
  - stale proof (proved_at_hash != current tip) still valid with explicit blockhash
  - invalid hex input
  - invalid proof structure
  - oversized proof (exceeds MAX_PROOF_SIZE_BYTES)
  - invalid verbosity level
"""

import time

import pytest
from requests.exceptions import HTTPError

NUM_BLOCKS = 10
TIMEOUT_SECONDS = 120


def wait_for_floresta_sync(florestad, target_height):
    """Poll floresta until it reaches the target height and exits IBD."""
    timeout = time.time() + TIMEOUT_SECONDS
    while time.time() < timeout:
        info = florestad.rpc.get_blockchain_info()
        if info["height"] == target_height and not info["ibd"]:
            return
        time.sleep(1)
    raise AssertionError(
        f"Floresta failed to sync to height {target_height} within {TIMEOUT_SECONDS}s"
    )


def get_coinbase_txid(utreexod, height):
    """Get the coinbase txid at a given block height from utreexod."""
    block_hash = utreexod.rpc.get_blockhash(height)
    block = utreexod.rpc.perform_request("getblock", [block_hash, 1])
    return block["tx"][0]


@pytest.mark.rpc
def test_verifyutxochaintipinclusionproof_valid(
    setup_logging, florestad_bitcoind_utreexod_with_chain
):
    """Valid proof returns True for verbosity=0 and a detailed object for verbosity=1."""
    log = setup_logging
    florestad, _, utreexod = florestad_bitcoind_utreexod_with_chain(NUM_BLOCKS)

    log.info(f"Waiting for floresta to sync to height {NUM_BLOCKS}...")
    wait_for_floresta_sync(florestad, NUM_BLOCKS)

    txid = get_coinbase_txid(utreexod, 1)
    log.info(f"Generating proof for coinbase tx {txid} at vout 0")
    proof_response = utreexod.rpc.proveutxochaintipinclusion([txid], [0])
    proof_hex = proof_response["hex"]

    # Verbosity 0: boolean result
    log.info("Verifying proof with verbosity=0")
    result_v0 = florestad.rpc.perform_request(
        "verifyutxochaintipinclusionproof", [proof_hex, 0]
    )
    assert result_v0 is True, f"Expected True, got {result_v0}"

    # Verbosity 1: detailed object
    log.info("Verifying proof with verbosity=1")
    result_v1 = florestad.rpc.perform_request(
        "verifyutxochaintipinclusionproof", [proof_hex, 1]
    )

    assert isinstance(result_v1, dict), f"Expected dict, got {type(result_v1)}"
    assert result_v1["valid"] is True
    assert "proved_at_hash" in result_v1
    assert "targets" in result_v1
    assert "num_proof_hashes" in result_v1
    assert "proof_hashes" in result_v1
    assert "hashes_proven" in result_v1

    best_hash = florestad.rpc.get_bestblockhash()
    assert result_v1["proved_at_hash"] == best_hash

    # targets: exactly 1 element (we proved 1 UTXO), each a non-negative integer
    assert len(result_v1["targets"]) == 1
    assert isinstance(result_v1["targets"][0], int)
    assert result_v1["targets"][0] >= 0

    # num_proof_hashes must match the actual proof_hashes list length
    assert result_v1["num_proof_hashes"] == len(result_v1["proof_hashes"])

    # proof_hashes: non-empty list of 64-char hex strings
    assert len(result_v1["proof_hashes"]) > 0
    for h in result_v1["proof_hashes"]:
        assert isinstance(h, str) and len(h) == 64, f"bad proof hash: {h}"
        int(h, 16)  # raises ValueError if not valid hex

    # hashes_proven: exactly 1 element (we proved 1 UTXO), 64-char hex
    assert len(result_v1["hashes_proven"]) == 1
    h = result_v1["hashes_proven"][0]
    assert isinstance(h, str) and len(h) == 64, f"bad proven hash: {h}"
    int(h, 16)  # raises ValueError if not valid hex


@pytest.mark.rpc
def test_verifyutxochaintipinclusionproof_with_blockhash(
    setup_logging, florestad_bitcoind_utreexod_with_chain
):
    """Proof verified against an explicit blockhash succeeds, and fails against a wrong one."""
    log = setup_logging
    florestad, _, utreexod = florestad_bitcoind_utreexod_with_chain(NUM_BLOCKS)

    log.info(f"Waiting for floresta to sync to height {NUM_BLOCKS}...")
    wait_for_floresta_sync(florestad, NUM_BLOCKS)

    tip_hash = florestad.rpc.get_bestblockhash()
    txid = get_coinbase_txid(utreexod, 1)
    log.info(f"Generating proof for coinbase tx {txid}")
    proof_response = utreexod.rpc.proveutxochaintipinclusion([txid], [0])
    proof_hex = proof_response["hex"]

    # Verify against the correct explicit blockhash
    log.info(f"Verifying proof against explicit blockhash {tip_hash}")
    result = florestad.rpc.perform_request(
        "verifyutxochaintipinclusionproof", [proof_hex, 0, tip_hash]
    )
    assert result is True

    # Verify against a wrong blockhash should fail
    wrong_hash = florestad.rpc.get_blockhash(1)
    log.info(f"Verifying proof against wrong blockhash {wrong_hash} should fail")
    with pytest.raises(HTTPError):
        florestad.rpc.perform_request(
            "verifyutxochaintipinclusionproof", [proof_hex, 0, wrong_hash]
        )

    # Mine more blocks and verify the proof is still valid with the original blockhash
    log.info("Mining more blocks to advance the chain...")
    utreexod.rpc.generate(5)

    log.info(f"Waiting for floresta to sync to height {NUM_BLOCKS + 5}...")
    wait_for_floresta_sync(florestad, NUM_BLOCKS + 5)

    log.info(f"Verifying proof still valid with original blockhash {tip_hash}")
    result = florestad.rpc.perform_request(
        "verifyutxochaintipinclusionproof", [proof_hex, 0, tip_hash]
    )
    assert result is True


@pytest.mark.rpc
def test_verifyutxochaintipinclusionproof_stale_with_blockhash(
    setup_logging, florestad_bitcoind_utreexod_with_chain
):
    """
    A proof becomes stale after new blocks are mined, but still validates
    when the original blockhash is explicitly provided.
    """
    log = setup_logging
    florestad, _, utreexod = florestad_bitcoind_utreexod_with_chain(NUM_BLOCKS)

    log.info(f"Waiting for floresta to sync to height {NUM_BLOCKS}...")
    wait_for_floresta_sync(florestad, NUM_BLOCKS)

    old_tip = florestad.rpc.get_bestblockhash()
    txid = get_coinbase_txid(utreexod, 1)
    log.info(f"Generating proof for coinbase tx {txid}")
    proof_response = utreexod.rpc.proveutxochaintipinclusion([txid], [0])
    proof_hex = proof_response["hex"]

    log.info("Mining more blocks to make the proof stale...")
    utreexod.rpc.generate(5)

    log.info(f"Waiting for floresta to sync to height {NUM_BLOCKS + 5}...")
    wait_for_floresta_sync(florestad, NUM_BLOCKS + 5)

    # Stale proof fails without explicit blockhash (defaults to current tip)
    log.info("Verifying stale proof without blockhash should fail")
    with pytest.raises(HTTPError):
        florestad.rpc.perform_request("verifyutxochaintipinclusionproof", [proof_hex])

    # Same proof succeeds when original blockhash is specified
    log.info(f"Verifying stale proof with original blockhash {old_tip} should succeed")
    result = florestad.rpc.perform_request(
        "verifyutxochaintipinclusionproof", [proof_hex, 0, old_tip]
    )
    assert result is True


@pytest.mark.rpc
def test_verifyutxochaintipinclusionproof_invalid_hex(florestad_node):
    """Non-hex string should fail."""
    with pytest.raises(HTTPError):
        florestad_node.rpc.perform_request(
            "verifyutxochaintipinclusionproof", ["not_valid_hex!!"]
        )


@pytest.mark.rpc
def test_verifyutxochaintipinclusionproof_invalid_proof(florestad_node):
    """Well-formed hex that doesn't decode to a valid proof should fail."""
    # 64 hex chars = 32 bytes, enough for a block hash but truncated as a proof
    garbage_proof = "aa" * 32
    with pytest.raises(HTTPError):
        florestad_node.rpc.perform_request(
            "verifyutxochaintipinclusionproof", [garbage_proof]
        )


@pytest.mark.rpc
def test_verifyutxochaintipinclusionproof_oversized_proof(florestad_node):
    """Proof exceeding MAX_PROOF_SIZE_BYTES should be rejected."""
    # MAX_PROOF_SIZE_BYTES = 32 + 9 + 24_386 * 9 + 9 + (24_386 * 64) * 32 + 4 + 24_386 * 32
    #                      = 50_942_408
    max_size = 50_942_408
    oversized_proof = "aa" * (max_size + 1)
    with pytest.raises(HTTPError):
        florestad_node.rpc.perform_request(
            "verifyutxochaintipinclusionproof", [oversized_proof]
        )


@pytest.mark.rpc
def test_verifyutxochaintipinclusionproof_invalid_verbosity(
    setup_logging, florestad_bitcoind_utreexod_with_chain
):
    """Valid proof with verbosity=2 should fail."""
    log = setup_logging
    florestad, _, utreexod = florestad_bitcoind_utreexod_with_chain(NUM_BLOCKS)

    log.info(f"Waiting for floresta to sync to height {NUM_BLOCKS}...")
    wait_for_floresta_sync(florestad, NUM_BLOCKS)

    txid = get_coinbase_txid(utreexod, 1)
    log.info(f"Generating proof for coinbase tx {txid}")
    proof_response = utreexod.rpc.proveutxochaintipinclusion([txid], [0])
    proof_hex = proof_response["hex"]

    log.info("Verifying proof with invalid verbosity=2")
    with pytest.raises(HTTPError):
        florestad.rpc.perform_request(
            "verifyutxochaintipinclusionproof", [proof_hex, 2]
        )
