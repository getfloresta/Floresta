# SPDX-License-Identifier: MIT OR Apache-2.0

"""
getblockhash.py

This functional test cli utility to interact with a Floresta node with `getblockhash`
"""

import pytest
from test_framework.constants import GENESIS_BLOCK_HASH

MINED_BLOCKS = 10


@pytest.mark.rpc
def test_get_block_hash(florestad_utreexod, node_manager):
    """
    Test the `getblockhash` shows the block hash.
    """
    florestad, utreexod = florestad_utreexod

    # Get initial block hashes
    initial_florestad_hash = florestad.rpc.get_blockhash(0)
    initial_utreexod_hash = utreexod.rpc.get_blockhash(0)

    assert initial_florestad_hash == initial_utreexod_hash == GENESIS_BLOCK_HASH

    # Mine blocks with utreexod
    utreexod.rpc.generate(MINED_BLOCKS)
    node_manager.wait_for_sync(florestad, MINED_BLOCKS)

    # Get final block hashes
    final_florestad_hash = florestad.rpc.get_blockhash(MINED_BLOCKS)
    final_utreexod_hash = utreexod.rpc.get_blockhash(MINED_BLOCKS)

    assert final_florestad_hash == final_utreexod_hash
