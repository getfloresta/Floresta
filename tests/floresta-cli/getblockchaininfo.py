"""
floresta_cli_getblockchainfo.py

This functional test cli utility to interact with a Floresta node with `getblockchaininfo`
"""

from test_framework import FlorestaTestFramework, NodeType


class GetBlockchaininfoTest(FlorestaTestFramework):
    """
    Test `getblockchaininfo` with a fresh node and its first block
    """

    nodes = [-1]
    best_block = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
    difficulty = 1
    height = 0
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
        self.assertEqual(response["best_block"], GetBlockchaininfoTest.best_block)
        self.assertEqual(response["difficulty"], GetBlockchaininfoTest.difficulty)
        self.assertEqual(response["height"], GetBlockchaininfoTest.height)
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
