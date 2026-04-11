# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Test JSON-RPC username/password authentication on florestad.
"""

from requests.exceptions import HTTPError
from test_framework import FlorestaTestFramework
from test_framework.daemon import ConfigP2P
from test_framework.electrum import ConfigElectrum
from test_framework.node import NodeType
from test_framework.rpc import ConfigRPC


class RpcAuthTest(FlorestaTestFramework):
    """
    Ensure RPC requests are rejected without credentials and accepted with credentials.
    """

    def set_test_params(self):
        rpc_config = ConfigRPC(
            host="127.0.0.1",
            port=18442,
            user="alice",
            password="secret",
        )
        self.florestad = self.add_node(
            variant=NodeType.FLORESTAD,
            rpc_config=rpc_config,
            p2p_config=ConfigP2P(host="127.0.0.1", port=0),
            extra_args=[],
            electrum_config=ConfigElectrum(host="127.0.0.1", port=0, tls=None),
            tls=False,
        )

    def run_test(self):
        self.run_node(self.florestad)

        authenticated = self.florestad.rpc.get_blockchain_info()
        self.assertEqual(authenticated["chain"], "regtest")

        host = self.florestad.rpc.config.host
        port = self.florestad.rpc.config.port
        self.florestad.rpc.set_config(ConfigRPC(host=host, port=port))

        with self.assertRaises(HTTPError):
            self.florestad.rpc.get_blockchain_info()


if __name__ == "__main__":
    RpcAuthTest().main()
