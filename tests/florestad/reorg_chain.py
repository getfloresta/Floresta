# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Chain reorg test

This test will spawn a florestad and a utreexod, we will use utreexod to mine some blocks.
Then we will invalidate one of those blocks, and mine an alternative chain. This should
make florestad switch to the new chain. We then compare the two node's main chain and
accumulator to make sure they are the same.
"""

import pytest

MINE_BLOCKS = 10
EXTRA_BLOCKS = 5


@pytest.mark.florestad
def test_reorg_chain(setup_logging, florestad_utreexod, node_manager):
    """Mine blocks, trigger a reorg and assert both nodes end up on the same chain."""
    log = setup_logging
    florestad, utreexod = florestad_utreexod

    node_manager.generate_blocks_and_sync(MINE_BLOCKS, is_finished_ibd=False)

    old_best_block_hash = florestad.rpc.get_bestblockhash()

    utreexo_block = utreexod.rpc.get_block_count()
    count_invalid_block = 5
    height_invalid = utreexo_block - count_invalid_block
    hash_invalid = utreexod.rpc.get_blockhash(height_invalid)
    utreexod.rpc.invalidate_block(hash_invalid)

    assert utreexod.rpc.get_block_count() < height_invalid
    log.info(f"Utreexod node has {utreexod.rpc.get_block_count()} blocks")
    log.info(f"Florestad node has {florestad.rpc.get_block_count()} blocks")

    log.info(f"Mining {count_invalid_block + EXTRA_BLOCKS} blocks to trigger reorg")
    node_manager.generate_blocks_and_sync(
        count_invalid_block + EXTRA_BLOCKS, is_finished_ibd=False
    )

    assert old_best_block_hash != florestad.rpc.get_bestblockhash()
    split_block_hash = florestad.rpc.get_blockhash(height_invalid)
    assert split_block_hash != hash_invalid

    florestad_info = florestad.rpc.get_blockchain_info()
    utreexod_info = utreexod.rpc.get_blockchain_info()
    assert florestad_info["bestblockhash"] == utreexod_info["bestblockhash"]
    assert florestad_info["headers"] == utreexod_info["blocks"]
