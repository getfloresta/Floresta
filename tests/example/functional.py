"""
functional-test.py

This is an example of how a functional-test should look like,
see `tests/test_framework/test_framework.py` for more info.
"""

from test_framework import FlorestaTestFramework, NodeType


class FunctionalTest(FlorestaTestFramework):
    """
    Tests should be a child class from FlorestaTestFramework

    In each test class definition, `set_test_params` and `run_test`, say what
    the test do and the expected result in the docstrings
    """

    expected_height = 0
    expected_block = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
    expected_difficulty = 1
    expected_leaf_count = 0

    def set_test_params(self):
        """
        Here we define setup for test adding a node definition
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

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
        # in this case, `florestad`, and wait for
        # all ports opened by it, including the
        # RPC port to be available
        self.run_node(self.florestad)

        # Once the node is running, we can create
        # a request to the RPC server. In this case, we
        # call it node, but in truth, will be a RPC request
        # to perform some kind of action
        inf_response = self.florestad.rpc.get_blockchain_info()

        # Make assertions with our framework. Avoid usage of
        # native `assert` clauses. For more information, see
        # https://github.com/vinteumorg/Floresta/issues/426
        self.assertEqual(inf_response["height"], FunctionalTest.expected_height)
        self.assertEqual(inf_response["best_block"], FunctionalTest.expected_block)
        self.assertEqual(inf_response["difficulty"], FunctionalTest.expected_difficulty)
        self.assertEqual(inf_response["leaf_count"], FunctionalTest.expected_leaf_count)

        # stop nodes
        self.stop()


if __name__ == "__main__":
    FunctionalTest().main()
