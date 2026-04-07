# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getaddrmaninfo.py

This functional test verifies the `getaddrmaninfo` RPC method.
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class GetAddrManInfoTest(FlorestaTestFramework):
    """
    Test `getaddrmaninfo` returns address manager statistics by network.
    """

    def set_test_params(self):
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

    def run_test(self):
        self.run_node(self.florestad)

        # Get initial stats
        info = self.florestad.rpc.get_addrman_info()
        self.assertIsSome(info)

        # Verify structure has all expected network keys
        for key in ["all_networks", "ipv4", "ipv6", "onion", "i2p", "cjdns"]:
            self.assertIn(key, info)
            self.assertIn("total", info[key])
            self.assertIn("new", info[key])
            self.assertIn("tried", info[key])

        initial_total = info["all_networks"]["total"]

        # Add a peer address and verify counts change
        self.florestad.rpc.add_peer_address("8.8.8.8", 8333, tried=True)

        info = self.florestad.rpc.get_addrman_info()
        self.assertEqual(info["all_networks"]["total"], initial_total + 1)
        self.assertEqual(info["ipv4"]["tried"], 1)


if __name__ == "__main__":
    GetAddrManInfoTest().main()
