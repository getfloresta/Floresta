# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Tests for block headers retrieval and validation in the Electrum server.

This tests the blockchain.block.headers endpoint which returns a chunk
of block headers from the main chain.
"""

import random
import pytest
from test_framework.util import wait_until

MINE_BLOCKS = 100
MAX_HEADERS = 2016


@pytest.mark.electrum
def test_block_headers(setup_logging, florestad_utreexod):
    """Test block headers retrieval and validation."""
    log = setup_logging
    florestad, utreexod = florestad_utreexod

    utreexod.rpc.generate(MINE_BLOCKS)
    wait_until(lambda: florestad.rpc.get_block_count() == MINE_BLOCKS)

    log.info("Testing out-of-range request...")
    response = florestad.electrum.block_headers(MINE_BLOCKS + 1, 10)
    assert response["count"] == 0
    assert response["headers"] == []
    assert response["max"] == MAX_HEADERS

    log.info("Testing invalid parameters...")
    with pytest.raises(ValueError):
        florestad.electrum.block_headers(-1, 10)

    with pytest.raises(ValueError):
        florestad.electrum.block_headers(0, -1)

    log.info("Testing valid requests...")
    compare_headers_range(florestad, 0, 10)
    compare_headers_range(florestad, MINE_BLOCKS - 10, 10)

    log.info("Testing random range...")
    random_start = random.randint(1, MINE_BLOCKS - 10)
    compare_headers_range(florestad, random_start, 10)

    log.info("Testing max count limit...")
    response = florestad.electrum.block_headers(0, MAX_HEADERS * 2)
    assert response["count"] <= response["max"]
    assert response["max"] == MAX_HEADERS
    assert response["count"] <= MAX_HEADERS


def compare_headers_range(florestad, start_height, count):
    """Compare block headers from Electrum and RPC for a range."""
    response = florestad.electrum.block_headers(start_height, count)

    # Validate response structure
    assert "count" in response
    assert "headers" in response
    assert "max" in response
    assert response["count"] == len(response["headers"])
    assert response["max"] == MAX_HEADERS

    # Compare each header with RPC
    for i, header in enumerate(response["headers"]):
        height = start_height + i
        block_hash = florestad.rpc.get_blockhash(height)
        rpc_header = florestad.rpc.get_blockheader(block_hash, verbosity=False)

        assert header == rpc_header
