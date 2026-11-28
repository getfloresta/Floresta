# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Test the --addnode cli option of florestad

This test will start a bitcoind, then start a florestad node with
the --addnode option pointing to the bitcoind node. Then check if
the bitcoind node is connected to the florestad node.
"""

import pytest

from test_framework.node import NodeType


@pytest.mark.florestad
def test_addnode(bitcoind_node, add_node_with_extra_args, node_manager):
    """
    Test the --addnode flag of florestad.
    """
    bitcoind = bitcoind_node
    florestad = add_node_with_extra_args(
        variant=NodeType.FLORESTAD,
        extra_args=[f"--addnode={bitcoind.p2p_url}"],
    )

    node_manager.wait_for_peers_connections(florestad, bitcoind)
