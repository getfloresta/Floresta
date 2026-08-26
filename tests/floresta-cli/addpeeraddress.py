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

    # IPv6: adding a routable IPv6 address with a separate port should succeed.
    addrman_ipv6_before = florestad_node.rpc.get_addrman_info()["ipv6"]["new"]
    result = florestad_node.rpc.add_peer_address("2001:4860:4860::8888", 8333)
    assert result["success"] is True
    assert addrman_ipv6_before < florestad_node.rpc.get_addrman_info()["ipv6"]["new"]

    # IPv6 without port: should use the network default
    addrman_ipv6_before = florestad_node.rpc.get_addrman_info()["ipv6"]["new"]
    result = florestad_node.rpc.add_peer_address("2001:4860:4860::8844")
    assert result["success"] is True
    assert addrman_ipv6_before < florestad_node.rpc.get_addrman_info()["ipv6"]["new"]

    # Onion: adding a valid .onion v3 address should succeed
    onion_addr = "ejgeimjypsfuijpxzy5xpwmmjmkr4izwze6od5pw74csjglflib6nsid.onion"
    addrman_onion_before = florestad_node.rpc.get_addrman_info()["onion"]["new"]
    result = florestad_node.rpc.add_peer_address(onion_addr, 8333)
    assert result["success"] is True
    assert addrman_onion_before < florestad_node.rpc.get_addrman_info()["onion"]["new"]
