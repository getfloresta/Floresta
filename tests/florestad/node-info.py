"""
Tests for node information exchanged between Floresta and other peers.
"""

import time
import re

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class NodeInfoTest(FlorestaTestFramework):
    def set_test_params(self):
        """
        Setup a florestad and a bitcoind node
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def run_test(self):
        """
        Tests that the node information (e.g., version and subversion) sent by Floresta to other
        peers is correct.
        """
        self.run_node(self.bitcoind)
        self.run_node(self.florestad)

        self.connect_nodes(self.florestad, self.bitcoind)

        peer_info = self.bitcoind.rpc.get_peerinfo()

        self.assertEqual(len(peer_info), 1)
        self.assertEqual(peer_info[0]["services"], "0000000001000009")
        self.assertEqual(peer_info[0]["version"], 70016)
        self.assertMatch(
            peer_info[0]["subver"],
            # Regex allows versions with optional suffixes like /Floresta/0.8.0-145-g089c7de-dirty/
            re.compile(r"\/Floresta:\d+\.\d+\.\d+.*\/"),
        )
        self.assertEqual(peer_info[0]["inbound"], True)

        peer_info = self.florestad.rpc.get_peerinfo()
        self.assertEqual(peer_info[0]["address"], self.bitcoind.p2p_url)
        self.assertEqual(peer_info[0]["kind"], "manual")
        self.assertEqual(
            peer_info[0]["services"],
            "ServiceFlags(NETWORK|WITNESS|NETWORK_LIMITED|P2P_V2)",
        )
        self.assertEqual(peer_info[0]["transport_protocol"], "V2")
        self.assertMatch(
            peer_info[0]["user_agent"],
            re.compile(r"\/Satoshi:\d*\.\d*\.\d*\/"),
        )


if __name__ == "__main__":
    NodeInfoTest().main()
