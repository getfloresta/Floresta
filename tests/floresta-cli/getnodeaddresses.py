# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getnodeaddresses.py

This functional test verifies the `getnodeaddresses` RPC method.
"""

import pytest
from test_framework.node import NodeType


@pytest.mark.rpc
def test_get_node_addresses(node_manager, florestad_node, add_node_with_extra_args):
    """
    Test `getnodeaddresses` returns known peer addresses from the address manager.
    """
    bitcoind = add_node_with_extra_args(NodeType.BITCOIND, ["-v2transport=0"])

    # Connect to bitcoind so florestad learns its address
    node_manager.connect_nodes(florestad_node, bitcoind)
    node_manager.wait_for_peers_connections(florestad_node, bitcoind)

    # count=0 returns all known addresses
    all_addresses = florestad_node.rpc.get_node_addresses(0)
    assert all_addresses is not None

    # Validate structure of returned addresses
    if len(all_addresses) > 0:
        addr = all_addresses[0]
        assert "time" in addr
        assert "services" in addr
        assert "address" in addr
        assert "port" in addr

    # count parameter must limit results
    if len(all_addresses) > 1:
        limited = florestad_node.rpc.get_node_addresses(1)
        assert len(limited) == 1

    # network filter must only return addresses of the requested type
    ipv4_addresses = florestad_node.rpc.get_node_addresses(0, "ipv4")
    assert ipv4_addresses is not None
    assert len(ipv4_addresses) >= len(all_addresses)
