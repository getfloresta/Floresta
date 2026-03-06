"""
Chain reorg test

This test will spawn a florestad and a utreexod, we will use utreexod to mine some blocks.
Then we will invalidate one of those blocks, and mine an alternative chain. This should
make florestad switch to the new chain. We then compare the two node's main chain and
accumulator to make sure they are the same.
"""

import time
import pytest


@pytest.mark.florestad
def test_reorg_chain(setup_logging, florestad_utreexod):
    """Mine blocks, trigger a reorg and assert both nodes end up on the same chain."""
    log = setup_logging
    florestad, utreexod = florestad_utreexod

    ChainReorgTest(log, florestad, utreexod).run()


class ChainReorgTest:
    """Tests that Florestad follows Utreexod during a chain reorganization."""

    def __init__(self, log, florestad, utreexod):
        """
        Attributes initialized to satisfy static analysis; real values are
        provided by pytest fixtures.
        """
        self.log = log
        self.florestad = florestad
        self.utreexod = utreexod

    def run(self):
        """Mine blocks, trigger a reorg and assert both nodes end up on the same chain."""

        blocks = 10
        self.mine_blocks(blocks)

        old_best_block_hash = self.florestad.rpc.get_bestblockhash()

        utreexo_block = self.utreexod.rpc.get_block_count()
        count_invalid_block = 5
        height_invalid = utreexo_block - count_invalid_block
        hash_invalid = self.utreexod.rpc.get_blockhash(height_invalid)
        self.utreexod.rpc.invalidate_block(hash_invalid)

        assert self.utreexod.rpc.get_block_count() < height_invalid
        self.log.info(f"Utreexod node has {self.utreexod.rpc.get_block_count()} blocks")
        self.log.info(
            f"Florestad node has {self.florestad.rpc.get_block_count()} blocks"
        )

        self.mine_blocks(count_invalid_block + 5)

        assert old_best_block_hash != self.florestad.rpc.get_bestblockhash()
        split_block_hash = self.florestad.rpc.get_blockhash(height_invalid)
        assert split_block_hash != hash_invalid

        floresta_info = self.florestad.rpc.get_blockchain_info()
        utreexo_info = self.utreexod.rpc.get_blockchain_info()
        assert floresta_info["best_block"] == utreexo_info["bestblockhash"]
        assert floresta_info["height"] == utreexo_info["blocks"]

    def mine_blocks(self, blocks):
        """Request Utreexod to generate blocks and wait for Florestad to sync."""
        self.log.info(f"Utreexod node mine {blocks} blocks")
        self.utreexod.rpc.generate(blocks)

        end = time.time() + 30
        while time.time() < end:
            floresta_block = self.florestad.rpc.get_block_count()
            utreexo_block = self.utreexod.rpc.get_block_count()
            if floresta_block == utreexo_block:
                self.log.info(f"Nodes are in sync: {floresta_block} blocks")
                break

            time.sleep(1)

        if floresta_block != utreexo_block:
            pytest.fail("Florestad node did not sync with Utreexod node in time")
