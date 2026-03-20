# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getaddednodeinfo.py

This functional test verifies the `getaddednodeinfo` RPC method.
"""

import pytest
from test_framework.node import NodeType


@pytest.mark.rpc
def test_get_added_node_info(node_manager, florestad_node, add_node_with_extra_args):
    """
    Test `getaddednodeinfo` returns correct information about manually added nodes.
    """
    bitcoind = add_node_with_extra_args(NodeType.BITCOIND, ["-v2transport=0"])

    # No added nodes yet
    info = florestad_node.rpc.get_added_node_info()
    assert isinstance(info, list)
    assert len(info) == 0

    # Add bitcoind as a manual peer
    florestad_node.rpc.addnode(bitcoind.p2p_url, "add", v2transport=False)
    node_manager.wait_for_peers_connections(florestad_node, bitcoind)

    # Should have 1 added node, connected
    info = florestad_node.rpc.get_added_node_info()
    assert len(info) == 1
    assert info[0]["addednode"] == bitcoind.p2p_url
    assert info[0]["connected"] is True

    # Remove the added node
    florestad_node.rpc.addnode(bitcoind.p2p_url, "remove")

    # Should have no added nodes
    info = florestad_node.rpc.get_added_node_info()
    assert len(info) == 0
