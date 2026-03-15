"""
Chain reorg test

This test will spawn a florestad and a utreexod, we will use utreexod to mine some blocks.
Then we will invalidate one of those blocks, and mine an alternative chain. This should
make florestad switch to the new chain. We then compare the two node's main chain and
accumulator to make sure they are the same.
"""

import re
import time

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.constants import WALLET_ADDRESS


class ChainReorgTest(FlorestaTestFramework):
    """Test the reorganization of the chain in florestad when using utreexod to mine blocks."""

    expected_chain = "regtest"

    def set_test_params(self):
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

        self.utreexod = self.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                f"--miningaddr={WALLET_ADDRESS}",
                "--utreexoproofindex",
                "--prune=0",
            ],
        )

    def run_test(self):
        # Start the nodes
        self.run_node(self.florestad)
        self.run_node(self.utreexod)

        # Mine some blocks with utreexod
        self.log("=== Mining blocks with utreexod")
        self.utreexod.rpc.generate(10)

        self.log("=== Connect floresta to utreexod")
        self.connect_nodes(self.florestad, self.utreexod)

        self.log("=== Wait for the nodes to sync...")
        time.sleep(20)

        self.log("=== Check that floresta has the same chain as utreexod.rpc...")
        floresta_chain = self.florestad.rpc.get_blockchain_info()
        utreexo_chain = self.utreexod.rpc.get_blockchain_info()
        self.assertEqual(floresta_chain["best_block"], utreexo_chain["bestblockhash"])
        self.assertEqual(floresta_chain["height"], utreexo_chain["blocks"])

        self.log("=== Get a block hash from utreexod to invalidate")
        hash = self.utreexod.rpc.get_blockhash(5)
        self.utreexod.rpc.invalidate_block(hash)

        self.log("=== Mining alternative chain with utreexod.rpc...")
        self.utreexod.rpc.generate(10)

        self.log("=== Wait for the nodes to sync")
        time.sleep(20)

        self.log("=== Check that floresta has switched to the new chain")
        floresta_chain = self.florestad.rpc.get_blockchain_info()
        utreexo_chain = self.utreexod.rpc.get_blockchain_info()
        self.assertEqual(floresta_chain["best_block"], utreexo_chain["bestblockhash"])
        self.assertEqual(floresta_chain["height"], utreexo_chain["blocks"])

        self.log("=== Compare the accumulator roots for each node")
        floresta_roots = self.florestad.rpc.get_roots()
        utreexo_roots = self.utreexod.rpc.get_utreexo_roots(
            utreexo_chain["bestblockhash"]
        )
        self.assertEqual(floresta_roots, utreexo_roots["roots"])


if __name__ == "__main__":
    ChainReorgTest().main()
