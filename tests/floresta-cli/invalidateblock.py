"""
invalidateblock.py

Test the `invalidateblock` RPC on florestad.

We mine blocks via utreexod, sync them to florestad, then call `invalidateblock`
on florestad to mark a block as invalid. We verify that florestad's tip rolls back
to the parent of the invalidated block.
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

        self.log("=== invalidateblock test passed")


if __name__ == "__main__":
    InvalidateBlockTest().main()
