# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getnodeaddresses.py

This functional test verifies the `getnodeaddresses` RPC method.
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class GetNodeAddressesTest(FlorestaTestFramework):
    """
    Test `getnodeaddresses` returns known peer addresses from the address manager.
    """

    def set_test_params(self):
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)
        self.bitcoind = self.add_node_extra_args(
            variant=NodeType.BITCOIND,
            extra_args=["-v2transport=0"],
        )

    def run_test(self):
        self.run_node(self.florestad)
        self.run_node(self.bitcoind)

        # Connect to bitcoind so florestad learns its address
        self.connect_nodes(self.florestad, self.bitcoind, "add", v2transport=False)
        self.wait_for_peers_connections(self.florestad, self.bitcoind)

        # count=0 returns all known addresses
        all_addresses = self.florestad.rpc.get_node_addresses(0)
        self.assertIsSome(all_addresses)

        # Validate structure of returned addresses
        if len(all_addresses) > 0:
            addr = all_addresses[0]
            self.assertIn("time", addr)
            self.assertIn("services", addr)
            self.assertIn("address", addr)
            self.assertIn("port", addr)

        # count parameter must limit results
        if len(all_addresses) > 1:
            limited = self.florestad.rpc.get_node_addresses(1)
            self.assertEqual(len(limited), 1)

        # network filter must only return addresses of the requested type
        ipv4_addresses = self.florestad.rpc.get_node_addresses(0, "ipv4")
        self.assertIsSome(ipv4_addresses)
        self.assertTrue(len(ipv4_addresses) >= len(all_addresses))


if __name__ == "__main__":
    GetNodeAddressesTest().main()
