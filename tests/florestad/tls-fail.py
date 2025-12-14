"""
florestad/tls-fail-test.py

This functional test checks the failure on connect to florestad's TLS port.
"""

import errno

from test_framework import FlorestaTestFramework, NodeType
from test_framework.electrum.client import ElectrumClient


class TestSslFailInitialization(FlorestaTestFramework):
    """
    Test that a request to the TLS Electrum port will fail if we don't enable it
    with `--enable-electrum-tls` and (`--generate-cert` or (`--tls-key-path` and `tls-cert-path`)).
    """

    electrum = None

    def set_test_params(self):
        """
        Instantiate the node without Electrum TLS.
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

    def run_test(self):
        """
        Run the node, create an Electrum client that will try to connect to
        the TLS port (20002), and assert that the connection was refused since TLS was not enabled.
        """
        self.run_node(self.florestad)

        # Create a connection with an Electrum client at the default Electrum TLS port.
        # It must fail since there is nothing bound to it.
        with self.assertRaises(ConnectionRefusedError) as exc:
            self.log("Trying to connect the Electrum no-TLS client")
            TestSslFailInitialization.electrum = ElectrumClient(
                self.florestad.config_electrum.host,
                self.florestad.config_electrum.port + 1,
            )

        self.log("Failed to connect to Electrum TLS client")
        self.assertIsSome(exc.exception)
        self.assertEqual(exc.exception.errno, errno.ECONNREFUSED)


if __name__ == "__main__":
    TestSslFailInitialization().main()
