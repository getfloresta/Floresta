# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Tests for block headers retrieval and validation in the Electrum server.

This tests the blockchain.block.headers endpoint which returns a chunk
of block headers from the main chain.
"""

import random
import pytest

from test_framework.bitcoin import BlockHeader

MINE_BLOCKS = 100
MAX_HEADERS = 2016


@pytest.mark.electrum
def test_block_headers(setup_logging, node_manager, florestad_utreexod):
    """Test block headers retrieval and validation."""
    log = setup_logging
    florestad, _ = florestad_utreexod

    node_manager.generate_blocks_and_sync(MINE_BLOCKS, is_finished_ibd=True)

    log.info("Testing out-of-range request...")
    response = florestad.electrum.block_headers(MINE_BLOCKS + 2, 10)
    assert response["count"] == 0
    assert response["hex"] == ""
    assert response["max"] == MAX_HEADERS

    log.info("Testing invalid parameters...")
    with pytest.raises(ValueError):
        florestad.electrum.block_headers(-1, 10)
    with pytest.raises(ValueError):
        florestad.electrum.block_headers(0, -1)

    log.info("Testing block headers starting at height 0...")
    compare_headers_range(florestad, 0, 10)

    log.info(f"Testing block headers starting at height {MINE_BLOCKS - 10}...")
    compare_headers_range(florestad, MINE_BLOCKS - 10, 10)

    random_start = random.randint(1, MINE_BLOCKS - 10)
    log.info(f"Testing random range starting at height {random_start}...")
    compare_headers_range(florestad, random_start, 10)

    log.info("Testing max count limit...")
    response = florestad.electrum.block_headers(0, MAX_HEADERS * 2)
    assert response["count"] <= response["max"]
    assert response["max"] == MAX_HEADERS
    assert response["count"] <= MAX_HEADERS


def compare_headers_range(florestad, start_height, count):
    """Helper function to compare block headers from Electrum and RPC for a given range."""
    response = florestad.electrum.block_headers(start_height, count)

    assert "count" in response
    assert "hex" in response
    assert "max" in response
    assert response["max"] == MAX_HEADERS

    headers_hex = response["hex"]
    header_size_hex = 160  # 80 bytes por header
    assert len(headers_hex) % header_size_hex == 0

    headers = []
    for i in range(0, len(headers_hex), header_size_hex):
        chunk = headers_hex[i : i + header_size_hex]
        headers.append(BlockHeader.deserialize(bytes.fromhex(chunk)))

    assert len(headers) == response["count"]

    for i, header in enumerate(headers):
        height = start_height + i
        rpc_hash = florestad.rpc.get_blockhash(height)
        rpc_header_hex = florestad.rpc.get_blockheader(rpc_hash, verbosity=False)

        assert header.serialize().hex() == rpc_header_hex
        assert header.hash() == rpc_hash
