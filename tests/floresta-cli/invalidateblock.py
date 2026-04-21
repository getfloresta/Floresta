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

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class InvalidateBlockTest(FlorestaTestFramework):
    """Test that invalidateblock marks a block invalid and rolls back the tip."""

    def set_test_params(self):
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)
        self.utreexod = self.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                "--miningaddr=bcrt1q4gfcga7jfjmm02zpvrh4ttc5k7lmnq2re52z2y",
                "--utreexoproofindex",
                "--prune=0",
            ],
        )

    def run_test(self):
        self.run_node(self.florestad)
        self.run_node(self.utreexod)

        # Mine 10 blocks on utreexod and sync to florestad
        self.log("=== Mining 10 blocks with utreexod")
        self.utreexod.rpc.generate(10)

        self.log("=== Connecting florestad to utreexod")
        self.connect_nodes(self.florestad, self.utreexod)

        self.log("=== Waiting for sync")
        time.sleep(20)

        # Verify both nodes are synced
        floresta_info = self.florestad.rpc.get_blockchain_info()
        utreexo_info = self.utreexod.rpc.get_blockchain_info()
        self.assertEqual(floresta_info["height"], utreexo_info["blocks"])
        self.assertEqual(floresta_info["best_block"], utreexo_info["bestblockhash"])

        # Get the hash of block 5 and its parent (block 4)
        hash_at_5 = self.florestad.rpc.get_blockhash(5)
        hash_at_4 = self.florestad.rpc.get_blockhash(4)

        # Invalidate block 5 on florestad
        self.log(f"=== Invalidating block at height 5: {hash_at_5}")
        self.florestad.rpc.invalidate_block(hash_at_5)

        # Verify florestad's tip rolled back to block 4
        new_info = self.florestad.rpc.get_blockchain_info()
        self.assertEqual(new_info["height"], 4)
        self.assertEqual(new_info["best_block"], hash_at_4)

        # Now extend the alternative tip to assert floresta can correctly sync with the new chain.
        #
        # This asserts that invalidate_block doesnt let florestad on a broken state.
        self.log("=== Mining 5 more blocks on utreexod to extend the tip")
        # Sanity check, if this matches we can invalidate that same hash so utreexod is on the same state as floresta is.
        self.assertEqual(self.utreexod.rpc.get_blockhash(5), hash_at_5)
        self.utreexod.rpc.invalidate_block(hash_at_5)

        # Mine 2 new blocks, extending from hash_at_4.
        self.utreexod.rpc.generate(2)

        self.log("=== Waiting for florestad to sync new blocks")
        time.sleep(120)

        # Verify florestad picked up the new blocks
        floresta_info = self.florestad.rpc.get_blockchain_info()
        utreexo_info = self.utreexod.rpc.get_blockchain_info()
        self.assertEqual(floresta_info["height"], utreexo_info["blocks"])
        self.assertEqual(floresta_info["best_block"], utreexo_info["bestblockhash"])

        # Verify the accumulators match
        floresta_roots = self.florestad.rpc.get_roots()
        utreexo_roots = self.utreexod.rpc.get_utreexo_roots(
            utreexo_info["bestblockhash"]
        )
        self.assertEqual(floresta_roots, utreexo_roots["roots"])


if __name__ == "__main__":
    InvalidateBlockTest().main()
