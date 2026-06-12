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

    addrman_tried_count_before = florestad_node.rpc.get_addrman_info()["ipv4"]["new"]
    # Adding a new address with tried=false should succeed
    #  and we should see a new entry for new addresses.
    result = florestad_node.rpc.add_peer_address("8.8.4.4", 8333)
    assert result["success"] is True
    assert (
        addrman_tried_count_before
        < florestad_node.rpc.get_addrman_info()["ipv4"]["new"]
    )

    addrman_tried_count_before = florestad_node.rpc.get_addrman_info()["ipv4"]["tried"]

    # Add a routable address as "tried"
    result = florestad_node.rpc.add_peer_address("8.8.8.8", 8333, tried=True)
    assert result is not None
    assert result["success"] is True
    assert (
        addrman_tried_count_before
        < florestad_node.rpc.get_addrman_info()["ipv4"]["tried"]
    )

    addrman_before = florestad_node.rpc.get_addrman_info()["ipv4"]["tried"]

    # Adding a duplicate should return false
    result = florestad_node.rpc.add_peer_address("8.8.8.8", 8333)
    assert result["success"] is False
    assert addrman_before == florestad_node.rpc.get_addrman_info()["ipv4"]["tried"]

    # Adding an address without port should use the network default
    result = florestad_node.rpc.add_peer_address("1.1.1.1")
    assert result["success"] is True
