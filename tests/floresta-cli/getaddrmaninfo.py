# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getaddrmaninfo.py

This functional test verifies the `getaddrmaninfo` RPC method.
"""

import pytest


@pytest.mark.rpc
def test_get_addrman_info(florestad_node):
    """
    Test `getaddrmaninfo` returns address manager statistics by network.
    """
    # Get initial stats
    info = florestad_node.rpc.get_addrman_info()
    assert info is not None

    # Verify structure has all expected network keys
    for key in ["all_networks", "ipv4", "ipv6", "onion", "i2p", "cjdns"]:
        assert key in info
        assert "total" in info[key]
        assert "new" in info[key]
        assert "tried" in info[key]

    initial_total = info["all_networks"]["total"]

    # Add a peer address and verify counts change
    florestad_node.rpc.add_peer_address("8.8.8.8", 8333, tried=True)

    info = florestad_node.rpc.get_addrman_info()
    assert info["all_networks"]["total"] == initial_total + 1
    assert info["ipv4"]["tried"] == 1
