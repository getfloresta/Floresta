# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Tests for block header retrieval and validation in the Electrum server.

This tests the blockchain.block.header endpoint which returns a block header
at a given height, optionally with a merkle proof for verification.
"""

import random
import pytest
from test_framework.util import wait_until

MINE_BLOCKS = 100


@pytest.mark.electrum
def test_block_header(florestad_utreexod):
    """Test block header retrieval and validation."""
    florestad, utreexod = florestad_utreexod

    utreexod.rpc.generate(MINE_BLOCKS)
    wait_until(lambda: florestad.rpc.get_block_count() == MINE_BLOCKS)

    with pytest.raises(ValueError):
        florestad.electrum.block_header(MINE_BLOCKS + 1)

    with pytest.raises(ValueError):
        florestad.electrum.block_header(-1)

    compare_headers(florestad, 0)
    compare_headers(florestad, MINE_BLOCKS)

    random_height = random.randint(1, MINE_BLOCKS - 1)
    compare_headers(florestad, random_height)


def compare_headers(florestad, height):
    """Helper function to compare block headers from Electrum and RPC."""
    electrum_header = florestad.electrum.block_header(height)

    block_hash = florestad.rpc.get_blockhash(height)
    rpc_header = florestad.rpc.get_blockheader(block_hash, verbosity=False)

    assert electrum_header == rpc_header
