"""
utreexod-test.py

This is an example of how a tests with utreexo should look like,
see `tests/test_framework/test_framework.py` for more info.
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.constants import (
    TEST_CHAIN,
    GENESIS_BLOCK_HASH,
    GENESIS_BLOCK_DIFFICULTY_INT,
)


class UtreexodTest(FlorestaTestFramework):
    """
    Tests should be a child class from FlorestaTestFramework

    In each test class definition, `set_test_params` and `run_test`, say what
    the test do and the expected result in the docstrings
    """

    def set_test_params(self):
        """
        Here we define setup for test adding a node definition
        """
        self.utreexod = self.add_node_default_args(variant=NodeType.UTREEXOD)

    # All tests should override the run_test method
    def run_test(self):
        """
        Here we define the test itself:

        - creates a dummy rpc listening on default port
        - perform some requests to FlorestaRPC node
        - if any assertion fails, all nodes will be stopped
        - if no error occurs, all nodes will be stopped at the end
        """
        # Start a new node (this crate's binary)
        # This method start a defined daemon,
        # in this case, `utreexod`, and wait for
        # all ports opened by it, including the
        # RPC port to be available
        self.run_node(self.utreexod)

        # Once the node is running, we can create
        # a request to the RPC server. In this case, we
        # call it node, but in truth, will be a RPC request
        # to perform some kind of action
        utreexo_response = self.utreexod.rpc.get_blockchain_info()

        self.assertEqual(utreexo_response["chain"], TEST_CHAIN)
        self.assertEqual(utreexo_response["bestblockhash"], GENESIS_BLOCK_HASH)
        self.assertEqual(utreexo_response["difficulty"], GENESIS_BLOCK_DIFFICULTY_INT)

        self.stop()


if __name__ == "__main__":
    UtreexodTest().main()
