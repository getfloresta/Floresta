# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getaddednodeinfo.py

This functional test cli utility to interact with a Floresta node with `getaddednodeinfo`
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class GetAddedNodeInfoTest(FlorestaTestFramework):
    """
    Test `getaddednodeinfo` by adding a bitcoind peer via `addnode add`,
    then verifying the result of `getaddednodeinfo` contains the peer.
    """

    def set_test_params(self):
        """
        Setup florestad and bitcoind nodes
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)
        self.bitcoind = self.add_node_extra_args(
            variant=NodeType.BITCOIND,
            extra_args=["-v2transport=0"],
        )

    def run_test(self):
        """
        Test getaddednodeinfo returns correct info before and after adding a peer
        """
        self.run_node(self.florestad)
        self.run_node(self.bitcoind)

        # Before adding any node, the list should be empty
        result = self.florestad.rpc.get_addednodeinfo()
        self.assertIsSome(result)
        self.assertEqual(len(result), 0)

        # Add bitcoind as a persistent peer
        self.connect_nodes(self.florestad, self.bitcoind, "add", v2transport=False)
        self.wait_for_peers_connections(self.florestad, self.bitcoind, True)

        # getaddednodeinfo should now return one entry
        result = self.florestad.rpc.get_addednodeinfo()
        self.assertIsSome(result)
        self.assertEqual(len(result), 1)
        self.assertTrue(result[0]["connected"])
        self.assertEqual(len(result[0]["addresses"]), 1)
        self.assertEqual(result[0]["addresses"][0]["connected"], "outbound")

        # Query by specific node address
        node_addr = result[0]["addednode"]
        filtered = self.florestad.rpc.get_addednodeinfo(node_addr)
        self.assertIsSome(filtered)
        self.assertEqual(len(filtered), 1)
        self.assertEqual(filtered[0]["addednode"], node_addr)


if __name__ == "__main__":
    GetAddedNodeInfoTest().main()
