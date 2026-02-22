"""
Test the --connect cli option of florestad

This test will start a utreexod, then start a florestad node with
the --connect option pointing to the utreexod node. Then check if
the utreexod node is connected to the florestad node.
"""

import pytest

from test_framework.node import NodeType


@pytest.mark.florestad
def test_connect(utreexod_node, add_node_with_extra_args, node_manager):
    """
    Test the --connect flag of florestad.
    """
    utreexod = utreexod_node
    florestad = add_node_with_extra_args(
        variant=NodeType.FLORESTAD,
        extra_args=[f"--connect={utreexod.p2p_url}"],
    )

    node_manager.wait_for_peers_connections(florestad, utreexod)
