"""
getblockhash.py

This functional test cli utility to interact with a Floresta node with `getblockhash`
"""

import re
import time
from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.constants import WALLET_ADDRESS


class GetBlockhashTest(FlorestaTestFramework):
    """
    Test `getblockhash` with a fresh node and expected initial 0 height block.
    After that, it will mine some blocks with utreexod and check that
    the blockhashes match between floresta, utreexod, and bitcoind.
    """

    def set_test_params(self):
        """
        Setup a single node
        """
        name = self.__class__.__name__.lower()
        self.v2transport = False

        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

        self.utreexod = self.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                f"--miningaddr={WALLET_ADDRESS}",
                "--prune=0",
            ],
        )

        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def run_test(self):
        """
        Run JSONRPC and get the hash of heights 0 to 10.
        """
        self.log("=== Starting nodes...")
        self.run_node(self.florestad)
        self.run_node(self.utreexod)
        self.run_node(self.bitcoind)

        self.log("=== Mining blocks with utreexod")
        self.utreexod.rpc.generate(10)
        time.sleep(5)

        self.log("=== Connect floresta to utreexod")
        self.connect_nodes(self.florestad, self.utreexod)

        self.log("=== Connect bitcoind to utreexod")
        self.connect_nodes(self.bitcoind, self.utreexod)

        self.log("=== Wait for the nodes to sync...")
        time.sleep(5)

        self.log("=== Get the tip block")
        block_count = self.florestad.rpc.get_block_count()

        for i in range(0, block_count + 1):
            self.log(f"=== Check the correct blockhash for height {i}...")
            hash_floresta = self.florestad.rpc.get_blockhash(i)
            hash_utreexod = self.utreexod.rpc.get_blockhash(i)
            hash_bitcoind = self.bitcoind.rpc.get_blockhash(i)
            for _hash in [hash_utreexod, hash_bitcoind]:
                self.assertEqual(hash_floresta, _hash)


if __name__ == "__main__":
    GetBlockhashTest().main()
