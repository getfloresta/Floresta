"""
florestad/tls-test.py

This functional test tests the proper creation of a TLS port on florestad.
"""

from test_framework import FlorestaTestFramework, NodeType
from test_framework.electrum.client import ElectrumClient


class TestSslInitialization(FlorestaTestFramework):
    """
    Test the initialization of florestad with --tls-key-path and --tls-cert-path and
    a request from Electrum client to TLS port and its success.
    """

    electrum = None

    def set_test_params(self):
        """
        Setup a single node and a electrum client at port 20002
        """
        self.florestad = self.add_node_with_tls(variant=NodeType.FLORESTAD)

    def run_test(self):
        """
        Run the TLS node, create a electrum client that will try to connect to port 20002.
        Send a ping to make sure everything is working.
        """
        self.run_node(self.florestad)

        # request something to TLS port
        res = self.florestad.electrum.ping()
        result = res["result"]
        id = res["id"]
        jsonrpc = res["jsonrpc"]

        # if pinged, we should get a "null" in response
        self.assertIsNone(result)
        self.assertEqual(id, 0)
        self.assertEqual(jsonrpc, "2.0")


if __name__ == "__main__":
    TestSslInitialization().main()
