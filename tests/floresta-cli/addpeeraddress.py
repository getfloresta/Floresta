# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_addpeeraddress.py

This functional test verifies the `addpeeraddress` RPC method.
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class AddPeerAddressTest(FlorestaTestFramework):
    """
    Test `addpeeraddress` adds a peer address to the address manager.
    """

    def set_test_params(self):
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

    def run_test(self):
        self.run_node(self.florestad)

        # Add a routable address as "tried" so it appears in good addresses
        result = self.florestad.rpc.add_peer_address("8.8.8.8", 8333, tried=True)
        self.assertIsSome(result)
        self.assertTrue(result["success"])

        # Verify it appears in getnodeaddresses
        addresses = self.florestad.rpc.get_node_addresses(10)
        found = any(
            addr["address"] == "8.8.8.8" and addr["port"] == 8333 for addr in addresses
        )
        self.assertTrue(found)

        # Adding a duplicate should return false
        result = self.florestad.rpc.add_peer_address("8.8.8.8", 8333)
        self.assertFalse(result["success"])

        # Adding with tried=false should still succeed for a new address
        result = self.florestad.rpc.add_peer_address("8.8.4.4", 8333)
        self.assertTrue(result["success"])


if __name__ == "__main__":
    AddPeerAddressTest().main()
