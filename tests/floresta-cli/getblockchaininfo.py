"""
floresta_cli_getblockchainfo.py

This functional test cli utility to interact with a Floresta node with `getblockchaininfo`
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.constants import (
    GENESIS_BLOCK_HASH,
    GENESIS_BLOCK_DIFFICULTY_INT,
    GENESIS_BLOCK_HEIGHT,
)


class GetBlockchaininfoTest(FlorestaTestFramework):
    """
    Test `getblockchaininfo` with a fresh node and its first block
    """

    nodes = [-1]
    ibd = True
    latest_block_time = 1296688602
    latest_work = "0000000000000000000000000000000000000000000000000000000000000002"
    leaf_count = 0
    progress = 0
    root_count = 0
    root_hashes = []
    validated = 0

    def set_test_params(self):
        """
        Setup a single node
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

    def run_test(self):
        """
        Run JSONRPC server and get some data about blockchain with only regtest genesis block
        """
        # Start node
        self.run_node(self.florestad)

        # Test assertions
        response = self.florestad.rpc.get_blockchain_info()
        self.assertEqual(response["best_block"], GENESIS_BLOCK_HASH)
        self.assertEqual(response["difficulty"], GENESIS_BLOCK_DIFFICULTY_INT)
        self.assertEqual(response["height"], GENESIS_BLOCK_HEIGHT)
        self.assertEqual(response["ibd"], GetBlockchaininfoTest.ibd)
        self.assertEqual(
            response["latest_block_time"], GetBlockchaininfoTest.latest_block_time
        )
        self.assertEqual(response["latest_work"], GetBlockchaininfoTest.latest_work)
        self.assertEqual(response["leaf_count"], GetBlockchaininfoTest.leaf_count)
        self.assertEqual(response["progress"], GetBlockchaininfoTest.progress)
        self.assertEqual(response["root_count"], GetBlockchaininfoTest.root_count)
        self.assertEqual(response["root_hashes"], GetBlockchaininfoTest.root_hashes)
        self.assertEqual(response["validated"], GetBlockchaininfoTest.validated)


if __name__ == "__main__":
    GetBlockchaininfoTest().main()
