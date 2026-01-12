"""
electrum-test.py

This is an example of how a tests with integrated electrum should look like,
see `tests/test_framework/test_framework.py` for more info.
"""

import json

from test_framework import FlorestaTestFramework
from test_framework.electrum.client import ElectrumClient
from test_framework.rpc.floresta import REGTEST_RPC_SERVER


class ElectrumTest(FlorestaTestFramework):
    """
    Tests should be a child class from FlorestaTestFramework

    In each test class definition, `set_test_params` and `run_test`, say what
    the test do and the expected result in the docstrings
    """

    index = [-1]
    expected_version = ["Floresta 0.4.1", "1.4"]

    def set_test_params(self):
        """
        Here we define setup for test adding a node definition
        """
        ElectrumTest.index[0] = self.add_node(
            variant="florestad", rpcserver=REGTEST_RPC_SERVER
        )

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
        self.run_node(ElectrumTest.index[0])

        # Create an instance of the Electrum Client,
        # a small implementation of the electrum
        # protocol, to test our own electrum implementation

        electrum = ElectrumClient(
            REGTEST_RPC_SERVER["host"], REGTEST_RPC_SERVER["ports"]["electrum-server"]
        )
        rpc_response = electrum.get_version()

        # Make assertions with our framework. Avoid usage of
        # native `assert` clauses. For more information, see
        # https://github.com/vinteumorg/Floresta/issues/426
        self.assertEqual(rpc_response["result"][0], ElectrumTest.expected_version[0])
        self.assertEqual(rpc_response["result"][1], ElectrumTest.expected_version[1])

        self.stop_node(ElectrumTest.index[0])


if __name__ == "__main__":
    ElectrumTest().main()
