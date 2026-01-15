"""
Test the floresta's `getblockcount` before and after mining a few blocks with
utreexod. Then, assert that the command returns the same number of
`blocks` and `height/validated` fields given in `getblockchaininfo`
of utreexod/bitcoind and floresta, respectively"""

import time
import pytest


@pytest.mark.rpc
def test_get_block_count(florestad_utreexod):
    """
    Test the `getblockcount` shows the block count changes before and after mining.
    """
    florestad, utreexod = florestad_utreexod

    # Get initial block counts
    initial_florestad_count = florestad.rpc.get_block_count()
    initial_utreexod_count = utreexod.rpc.get_block_count()

    assert initial_florestad_count == initial_utreexod_count == 0

    # Mine blocks with utreexod
    utreexod.rpc.generate(10)
    timeout = time.time() + 20
    while time.time() < timeout:
        if florestad.rpc.get_block_count() == utreexod.rpc.get_block_count() == 10:
            break
        time.sleep(1)

    # Get final block counts
    final_florestad_count = florestad.rpc.get_block_count()
    final_utreexod_count = utreexod.rpc.get_block_count()

    assert final_florestad_count == final_utreexod_count == 10
