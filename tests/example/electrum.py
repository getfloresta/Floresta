"""
electrum-test.py

This is an example of how a tests with integrated electrum should look like,
see `tests/test_framework/test_framework.py` for more info.
"""

from test_framework import FlorestaTestFramework
from test_framework.electrum.client import ElectrumClient


class ElectrumTest(FlorestaTestFramework):
    """
    Tests should be a child class from FlorestaTestFramework

    In each test class definition, `set_test_params` and `run_test`, say what
    the test do and the expected result in the docstrings
    """

    index = [-1]
    expected_version = ["Floresta 0.4.0", "1.4"]

    def set_test_params(self):
        """
        Here we define setup for test adding a node definition
        """
        self.florestad = self.add_node(variant="florestad")

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

        # Create an instance of the Electrum Client,
        # a small implementation of the electrum
        # protocol, to test our own electrum implementation
        host = self.florestad.get_host()
        port = self.florestad.get_port("electrum-server")
        electrum = ElectrumClient(host, port)
        rpc_response = electrum.get_version()

        # Make assertions with our framework. Avoid usage of
        # native `assert` clauses. For more information, see
        # https://github.com/getfloresta/Floresta/issues/426
        self.assertEqual(rpc_response["result"][0], ElectrumTest.expected_version[0])
        self.assertEqual(rpc_response["result"][1], ElectrumTest.expected_version[1])

        self.stop()


if __name__ == "__main__":
    ElectrumTest().main()
