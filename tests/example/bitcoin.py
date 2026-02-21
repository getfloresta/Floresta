"""
bitcoin-test.py

This is an example of how a tests with bitcoin should look like,
see `tests/test_framework/test_framework.py` for more info.
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.constants import (
    GENESIS_BLOCK_HASH,
    GENESIS_BLOCK_DIFFICULTY_FLOAT,
    TEST_CHAIN,
)


class BitcoindTest(FlorestaTestFramework):
    """
    Tests should be a child class from FlorestaTestFramework

    In each test class definition, `set_test_params` and `run_test`, say what
    the test do and the expected result in the docstrings
    """

    def set_test_params(self):
        """
        Here we define setup for test adding a node definition
        """
        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    # All tests should override the run_test method
    def run_test(self):
        """
        Here we define the test itself:

        - creates a dummy rpc listening on default port
        - perform some requests to BitcoinRPC node
        - if any assertion fails, all nodes will be stopped
        - if no error occurs, all nodes will be stopped at the end
        """
        # Start a new node (the bitcoind's binary).
        # This method start a defined daemon,
        # in this case, `bitcoind`, and wait for
        # all ports opened by it, including the
        # RPC port to be available
        self.run_node(self.bitcoind)

        # Once the node is running, we can create
        # a request to the RPC server. In this case, we
        # call it node, but in truth, will be a RPC request
        # to perform some kind of action
        response = self.bitcoind.rpc.get_blockchain_info()

        self.assertEqual(response["chain"], TEST_CHAIN)
        self.assertEqual(response["bestblockhash"], GENESIS_BLOCK_HASH)
        self.assertEqual(response["difficulty"], GENESIS_BLOCK_DIFFICULTY_FLOAT)

        self.stop()


if __name__ == "__main__":
    BitcoindTest().main()
