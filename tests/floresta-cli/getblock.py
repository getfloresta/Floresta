"""
floresta_cli_getblock.py

This functional test cli utility to interact with a Floresta node with `getblock`
"""

import time
import random

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class GetBlockTest(FlorestaTestFramework):

    def set_test_params(self):

        self.v2transport = True
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)
        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def compare_block(self, height: int):
        block_hash = self.bitcoind.rpc.get_blockhash(height)
        self.log(f"Comparing block {block_hash} between florestad and bitcoind")

        self.log("Fetching request with verbosity 0")
        floresta_block = self.florestad.rpc.get_block(block_hash, 0)
        bitcoind_block = self.bitcoind.rpc.get_block(block_hash, 0)
        self.assertEqual(floresta_block, bitcoind_block)

        self.log("Fetching request with verbosity 1")
        floresta_block = self.florestad.rpc.get_block(block_hash, 1)
        bitcoind_block = self.bitcoind.rpc.get_block(block_hash, 1)

        for key, bval in bitcoind_block.items():
            fval = floresta_block[key]

            self.log(f"Comparing {key} field: floresta={fval} bitcoind={bval}")
            if key == "difficulty":
                # Allow small differences in floating point representation
                self.assertEqual(round(fval, 3), round(bval, 3))
            else:
                self.assertEqual(fval, bval)

    def run_test(self):
        self.run_node(self.florestad)
        self.run_node(self.bitcoind)

        self.bitcoind.rpc.generate_block(2017)
        time.sleep(1)
        self.bitcoind.rpc.generate_block(6)

        self.log("Connecting florestad to bitcoind")
        self.connect_nodes(self.florestad, self.bitcoind)

        block_count = self.bitcoind.rpc.get_block_count()
        end = time.time() + 20
        while time.time() < end:
            if self.florestad.rpc.get_block_count() == block_count:
                break
            time.sleep(0.5)

        self.assertEqual(
            self.florestad.rpc.get_block_count(), self.bitcoind.rpc.get_block_count()
        )

        self.log("Testing getblock RPC in the genesis block")
        self.compare_block(0)

        random_block = random.randint(1, block_count)
        self.log(f"Testing getblock RPC in block {random_block}")
        self.compare_block(random_block)

        self.log(f"Testing getblock RPC in block {block_count}")
        self.compare_block(block_count)


if __name__ == "__main__":
    GetBlockTest().main()
