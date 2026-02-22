"""
getblockhash.py

This functional test cli utility to interact with a Floresta node with `getblockhash`
"""

import time
import pytest

from conftest import GENESIS_BLOCK_BLOCK


@pytest.mark.rpc
def test_get_block_hash(florestad_utreexod):
    """
    Test the `getblockhash` shows the block hash.
    """
    florestad, utreexod = florestad_utreexod

    # Get initial block hashes
    initial_florestad_hash = florestad.rpc.get_blockhash(0)
    initial_utreexod_hash = utreexod.rpc.get_blockhash(0)

    assert initial_florestad_hash == initial_utreexod_hash == GENESIS_BLOCK_BLOCK

    # Mine blocks with utreexod
    utreexod.rpc.generate(10)
    timeout = time.time() + 15
    while time.time() < timeout:
        if florestad.rpc.get_block_count() == utreexod.rpc.get_block_count() == 10:
            break
        time.sleep(1)

    # Get final block hashes
    final_florestad_hash = florestad.rpc.get_blockhash(10)
    final_utreexod_hash = utreexod.rpc.get_blockhash(10)

    assert final_florestad_hash == final_utreexod_hash
