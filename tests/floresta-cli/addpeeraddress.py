# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_addpeeraddress.py

This functional test verifies the `addpeeraddress` RPC method.
"""

import pytest


@pytest.mark.rpc
def test_add_peer_address(florestad_node):
    """
    Test `addpeeraddress` adds a peer address to the address manager.
    """
    # Add a routable address as "tried"
    result = florestad_node.rpc.add_peer_address("8.8.8.8", 8333, tried=True)
    assert result is not None
    assert result["success"] is True

    # Adding a duplicate should return false
    result = florestad_node.rpc.add_peer_address("8.8.8.8", 8333)
    assert result["success"] is False

    # Adding a new address with tried=false should succeed
    result = florestad_node.rpc.add_peer_address("8.8.4.4", 8333)
    assert result["success"] is True
