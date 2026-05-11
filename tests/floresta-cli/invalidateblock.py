# SPDX-License-Identifier: MIT OR Apache-2.0

"""
invalidateblock.py

Test the `invalidateblock` RPC on florestad.

We mine blocks via utreexod, sync them to florestad, then call `invalidateblock`
on florestad to mark a block as invalid. We verify that florestad's tip rolls back
to the parent of the invalidated block.

After invalidation, we extend the tip on utreexod and verify that florestad can
still sync new blocks — this catches accumulator bugs where invalidate_block
leaves the in-memory acc stale, causing subsequent block validation to fail.
"""

import time

import pytest


def wait_for_sync(log, florestad, utreexod, timeout=30):
    """Poll until florestad and utreexod report the same block count."""
    end = time.time() + timeout
    while time.time() < end:
        florestad_block = florestad.rpc.get_block_count()
        utreexod_block = utreexod.rpc.get_block_count()
        if florestad_block == utreexod_block:
            log.info(f"Nodes are in sync: {florestad_block} blocks")
            return
        time.sleep(1)

    pytest.fail("Florestad did not sync with Utreexod in time")


@pytest.mark.florestad
def test_invalidateblock(setup_logging, florestad_utreexod):
    """Test that invalidateblock marks a block invalid and rolls back the tip."""
    log = setup_logging
    florestad, utreexod = florestad_utreexod

    # Mine 10 blocks on utreexod and sync to florestad
    log.info("Mining 10 blocks with utreexod")
    utreexod.rpc.generate(10)
    wait_for_sync(log, florestad, utreexod)

    # Verify both nodes are synced
    floresta_info = florestad.rpc.get_blockchain_info()
    utreexo_info = utreexod.rpc.get_blockchain_info()
    assert floresta_info["height"] == utreexo_info["blocks"]
    assert floresta_info["best_block"] == utreexo_info["bestblockhash"]

    # Get the hash of block 5 and its parent (block 4)
    hash_at_5 = florestad.rpc.get_blockhash(5)
    hash_at_4 = florestad.rpc.get_blockhash(4)

    # Invalidate block 5 on florestad
    log.info(f"Invalidating block at height 5: {hash_at_5}")
    florestad.rpc.invalidate_block(hash_at_5)

    # Verify florestad's tip rolled back to block 4
    new_info = florestad.rpc.get_blockchain_info()
    assert new_info["height"] == 4
    assert new_info["best_block"] == hash_at_4

    # Now extend the alternative tip to assert floresta can correctly sync
    # with the new chain.
    #
    # This asserts that invalidate_block doesn't leave florestad in a broken state.
    log.info("Mining new blocks on utreexod to extend the tip")

    # Sanity check: utreexod should have the same block at height 5
    assert utreexod.rpc.get_blockhash(5) == hash_at_5
    utreexod.rpc.invalidate_block(hash_at_5)

    # Mine 2 new blocks, extending from hash_at_4
    utreexod.rpc.generate(2)

    log.info("Waiting for florestad to sync new blocks")
    wait_for_sync(log, florestad, utreexod, timeout=120)

    # Verify florestad picked up the new blocks
    floresta_info = florestad.rpc.get_blockchain_info()
    utreexo_info = utreexod.rpc.get_blockchain_info()
    assert floresta_info["height"] == utreexo_info["blocks"]
    assert floresta_info["best_block"] == utreexo_info["bestblockhash"]

    # Verify the accumulators match
    floresta_roots = florestad.rpc.get_roots()
    utreexo_roots = utreexod.rpc.get_utreexo_roots(utreexo_info["bestblockhash"])
    assert floresta_roots == utreexo_roots["roots"]
