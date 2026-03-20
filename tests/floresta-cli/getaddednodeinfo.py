# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getaddednodeinfo.py

This functional test verifies the `getaddednodeinfo` RPC method.
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class GetAddedNodeInfoTest(FlorestaTestFramework):
    """
    Test `getaddednodeinfo` returns correct information about manually added nodes.
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

        # No added nodes yet
        info = self.florestad.rpc.get_added_node_info()
        self.assertIsSome(info)
        self.assertEqual(len(info), 0)

        # Add bitcoind as a manual peer
        self.florestad.rpc.addnode(self.bitcoind.p2p_url, "add", v2transport=False)
        self.wait_for_peers_connections(self.florestad, self.bitcoind)

        # Should have 1 added node, connected
        info = self.florestad.rpc.get_added_node_info()
        self.assertEqual(len(info), 1)
        self.assertEqual(info[0]["addednode"], self.bitcoind.p2p_url)
        self.assertTrue(info[0]["connected"])

        # Remove the added node
        self.florestad.rpc.addnode(self.bitcoind.p2p_url, "remove")

        # Should have no added nodes
        info = self.florestad.rpc.get_added_node_info()
        self.assertEqual(len(info), 0)


if __name__ == "__main__":
    GetAddedNodeInfoTest().main()
