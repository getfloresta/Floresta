# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getnodeaddresses.py

This functional test verifies the `getnodeaddresses` RPC method.

Note on test coverage: the address manager only tracks publicly routable
addresses (see ``AddressMan::push_addresses`` and ``LocalAddress::is_routable``
in ``address_man.rs``).  In regtest every peer runs on 127.0.0.1, which is
rejected by the routability filter, so connecting peers in this environment
does **not** populate the address manager.

TODO: once the test framework supports signet, extend these tests with concrete
addresses: connect to real peers, call ``getnodeaddresses``, and assert on the
returned address fields (time, services, address, port, network), count limiting,
and per-network filtering with actual data.
"""

import pytest
from requests.exceptions import HTTPError


@pytest.mark.rpc
def test_get_node_addresses_empty(florestad_node):
    """
    Test `getnodeaddresses` returns an empty list when no routable addresses
    are known (the regtest case).
    """
    # count=0 means "return all" -- empty addrman yields an empty list
    all_addresses = florestad_node.rpc.get_node_addresses(0)
    assert all_addresses == []

    # count=1 with no addresses still returns an empty list
    one_address = florestad_node.rpc.get_node_addresses(1)
    assert one_address == []

    # Filtering by supported networks still returns empty
    ipv4_addresses = florestad_node.rpc.get_node_addresses(0, "ipv4")
    assert ipv4_addresses == []

    ipv6_addresses = florestad_node.rpc.get_node_addresses(0, "ipv6")
    assert ipv6_addresses == []


@pytest.mark.rpc
def test_get_node_addresses_unsupported_networks(florestad_node):
    """
    Test `getnodeaddresses` rejects unsupported network filters with an error.
    """
    for network in ("onion", "i2p", "cjdns"):
        with pytest.raises(HTTPError):
            florestad_node.rpc.get_node_addresses(0, network)


@pytest.mark.rpc
def test_get_node_addresses_unknown_network(florestad_node):
    """
    Test `getnodeaddresses` rejects an unknown network string with an error.
    """
    with pytest.raises(HTTPError):
        florestad_node.rpc.get_node_addresses(0, "banana")
