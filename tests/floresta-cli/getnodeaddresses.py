# SPDX-License-Identifier: MIT OR Apache-2.0

"""
tests/floresta-cli/getnodeaddresses.py

This functional test verifies the `getnodeaddresses` RPC method.
It uses `addpeeraddress` to inject routable addresses into the address
manager and then verifies that `getnodeaddresses` returns them correctly.
"""

import pytest
from test_framework.messages import msg_addrv2
from test_framework.rpc.base import assert_rpc_error, make_request
from test_framework.util import wait_until

# Reusable test addresses across all tests
IPV4_ADDR = "8.8.8.8"
IPV4_ADDR_2 = "8.8.4.4"
IPV4_ADDR_3 = "1.1.1.1"
IPV6_ADDR = "2001:4860:4860::8888"
ONION_ADDR = "ejgeimjypsfuijpxzy5xpwmmjmkr4izwze6od5pw74csjglflib6nsid.onion"


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


@pytest.mark.rpc
def test_get_node_addresses_returns_injected(florestad_node):
    """
    Test that addresses injected via `addpeeraddress` are returned by
    `getnodeaddresses` with correct fields across IPv4, IPv6, and onion.
    """
    injected_ipv4 = [IPV4_ADDR, IPV4_ADDR_2, IPV4_ADDR_3]
    injected_ipv6 = [IPV6_ADDR]
    injected_onion = [ONION_ADDR]

    for addr in injected_ipv4 + injected_ipv6 + injected_onion:
        result = florestad_node.rpc.add_peer_address(addr, 8333)
        assert result["success"] is True

    addresses = florestad_node.rpc.get_node_addresses(0)
    total = len(injected_ipv4) + len(injected_ipv6) + len(injected_onion)
    assert len(addresses) == total

    returned_addrs = {a["address"] for a in addresses}
    assert set(injected_ipv4).issubset(returned_addrs)
    assert set(injected_ipv6).issubset(returned_addrs)
    assert set(injected_onion).issubset(returned_addrs)

    # Verify each entry has the expected fields and types
    for entry in addresses:
        assert isinstance(entry["time"], int)
        assert isinstance(entry["services"], int)
        assert isinstance(entry["address"], str)
        assert isinstance(entry["port"], int)
        assert entry["port"] == 8333
        assert entry["network"] in ("ipv4", "ipv6", "onion")


@pytest.mark.rpc
def test_get_node_addresses_count_limiting(florestad_node):
    """
    Test that the count parameter limits the number of returned addresses.
    """
    for addr in [IPV4_ADDR, IPV4_ADDR_2, IPV4_ADDR_3]:
        florestad_node.rpc.add_peer_address(addr, 8333)

    addresses = florestad_node.rpc.get_node_addresses(2)
    assert len(addresses) == 2

    # Default call (no count) returns 1 address
    default = florestad_node.rpc.get_node_addresses()
    assert len(default) == 1

    # count=0 returns all
    all_addresses = florestad_node.rpc.get_node_addresses(0)
    assert len(all_addresses) == 3


@pytest.mark.rpc
def test_get_node_addresses_network_filtering(florestad_node):
    """
    Test that filtering by network returns only addresses of that type.
    """
    # Inject one IPv4, one IPv6, and one onion
    florestad_node.rpc.add_peer_address(IPV4_ADDR, 8333)
    florestad_node.rpc.add_peer_address(IPV6_ADDR, 8333)
    florestad_node.rpc.add_peer_address(ONION_ADDR, 8333)

    ipv4_only = florestad_node.rpc.get_node_addresses(0, "ipv4")
    assert len(ipv4_only) == 1
    assert ipv4_only[0]["address"] == IPV4_ADDR
    assert ipv4_only[0]["network"] == "ipv4"

    ipv6_only = florestad_node.rpc.get_node_addresses(0, "ipv6")
    assert len(ipv6_only) == 1
    assert ipv6_only[0]["address"] == IPV6_ADDR
    assert ipv6_only[0]["network"] == "ipv6"

    onion_only = florestad_node.rpc.get_node_addresses(0, "onion")
    assert len(onion_only) == 1
    assert onion_only[0]["address"] == ONION_ADDR
    assert onion_only[0]["network"] == "onion"

    # All networks returns all three
    all_addresses = florestad_node.rpc.get_node_addresses(0)
    assert len(all_addresses) == 3


@pytest.mark.rpc
def test_get_node_addresses_unsupported_networks(florestad_node):
    """
    Test `getnodeaddresses` returns an empty list for valid-but-currently-unsupported
    network filters (i2p, cjdns), even when other addresses exist.
    """
    # Inject addresses from all supported networks so the addrman is not empty
    florestad_node.rpc.add_peer_address(IPV4_ADDR, 8333)
    florestad_node.rpc.add_peer_address(IPV6_ADDR, 8333)
    florestad_node.rpc.add_peer_address(ONION_ADDR, 8333)

    # Unsupported network filters return empty — none of the above addresses leak through
    for network in ("i2p", "cjdns"):
        result = florestad_node.rpc.get_node_addresses(0, network)
        assert result == []


@pytest.mark.rpc
def test_get_node_addresses_shuffled(florestad_node):
    """
    Test that `getnodeaddresses` shuffles results.

    With 5 injected addresses, repeated calls should eventually return them
    in a different order.  The probability that 10 consecutive calls all
    produce the identical permutation by chance is (1/5!)^9 ≈ 10^-19.
    """
    addrs = [IPV4_ADDR, IPV4_ADDR_2, IPV4_ADDR_3, IPV6_ADDR, ONION_ADDR]
    for addr in addrs:
        florestad_node.rpc.add_peer_address(addr, 8333)

    orderings = set()
    for _ in range(10):
        result = florestad_node.rpc.get_node_addresses(0)
        ordering = tuple(a["address"] for a in result)
        orderings.add(ordering)

    # At least two distinct orderings should appear
    assert (
        len(orderings) > 1
    ), "Expected shuffled results but all 10 calls returned the same order"


@pytest.mark.rpc
def test_get_node_addresses_unknown_network(florestad_node):
    """
    Test `getnodeaddresses` rejects an unknown network string with an error.
    """
    resp = make_request(florestad_node, "getnodeaddresses", params=[0, "banana"])
    assert_rpc_error(
        resp,
        expected_status_code=400,
        expected_message="Invalid parameter type",
    )


@pytest.mark.p2p
def test_get_node_addresses_via_p2p(node_manager, florestad_node):
    """
    Test that addresses relayed over P2P via addrv2 messages are stored
    in the address manager and returned by `getnodeaddresses`.

    This exercises the P2P address relay path instead of only relying on addpeeraddress.
    """
    p2p_conn = node_manager.add_p2p_connection_default(
        node=florestad_node,
        p2p_idx=0,
    )
    wait_until(
        predicate=lambda: florestad_node.rpc.get_connectioncount() == 1,
        error_msg="Floresta node did not accept the P2P connection",
    )

    # create_node_address generates a mix: IPv4, IPv6, onion, and i2p addresses
    addrs = node_manager.create_node_address(10)
    msg = msg_addrv2()
    msg.addrs = addrs

    p2p_conn.send_and_ping(msg)

    # The address manager accepts IPv4, IPv6, and TorV3 (SUPPORTED networks).
    # I2P and CJDNS are rejected. From 10 addresses with the create_node_address
    # distribution: i%5==0 → i2p (indices 0,5), i%3==0 → onion (indices 3,6,9),
    # i%2==0 → ipv6 (indices 2,4,8), else → ipv4 (indices 1,7).
    # That gives: 2 i2p (rejected), 3 onion, 3 ipv6, 2 ipv4 → 8 stored.
    addresses = florestad_node.rpc.get_node_addresses(0)
    assert len(addresses) == 8

    networks = {a["network"] for a in addresses}
    assert "ipv4" in networks
    assert "ipv6" in networks
    assert "onion" in networks

    # Verify filtering works on P2P-relayed addresses too
    ipv4_only = florestad_node.rpc.get_node_addresses(0, "ipv4")
    assert len(ipv4_only) == 2
    assert all(a["network"] == "ipv4" for a in ipv4_only)

    ipv6_only = florestad_node.rpc.get_node_addresses(0, "ipv6")
    assert len(ipv6_only) == 3
    assert all(a["network"] == "ipv6" for a in ipv6_only)

    onion_only = florestad_node.rpc.get_node_addresses(0, "onion")
    assert len(onion_only) == 3
    assert all(a["network"] == "onion" for a in onion_only)
