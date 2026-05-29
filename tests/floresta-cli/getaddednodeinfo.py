# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getaddednodeinfo.py

This functional test verifies the `getaddednodeinfo` RPC method.
"""

import pytest
from requests.exceptions import HTTPError
from test_framework.node import NodeType


@pytest.mark.rpc
def test_get_added_node_info_empty(florestad_node):
    """
    With no manually added nodes, getaddednodeinfo returns an empty list.
    """
    info = florestad_node.rpc.get_added_node_info()
    assert isinstance(info, list)
    assert len(info) == 0


@pytest.mark.rpc
def test_get_added_node_info_connected(
    node_manager, florestad_node, add_node_with_extra_args
):
    """
    After adding a node via addnode and connecting, getaddednodeinfo should
    report the node as connected with a populated addresses array.
    """
    bitcoind = add_node_with_extra_args(NodeType.BITCOIND, ["-v2transport=0"])

    florestad_node.rpc.addnode(bitcoind.p2p_url, "add", v2transport=False)
    node_manager.wait_for_peers_connections(florestad_node, bitcoind)

    info = florestad_node.rpc.get_added_node_info()
    assert len(info) == 1

    entry = info[0]
    assert entry["addednode"] == bitcoind.p2p_url
    assert entry["connected"] is True

    # The addresses array must be present and populated when connected
    assert "addresses" in entry
    assert len(entry["addresses"]) == 1
    assert entry["addresses"][0]["address"] == bitcoind.p2p_url
    assert entry["addresses"][0]["connected"] == "outbound"


@pytest.mark.rpc
def test_get_added_node_info_multiple_nodes(
    node_manager, florestad_node, add_node_with_extra_args
):
    """
    Adding multiple nodes via addnode should all appear in getaddednodeinfo.
    """
    bitcoind_a = add_node_with_extra_args(NodeType.BITCOIND, ["-v2transport=0"])
    bitcoind_b = add_node_with_extra_args(NodeType.BITCOIND, ["-v2transport=0"])

    florestad_node.rpc.addnode(bitcoind_a.p2p_url, "add", v2transport=False)
    florestad_node.rpc.addnode(bitcoind_b.p2p_url, "add", v2transport=False)
    node_manager.wait_for_peers_connections(florestad_node, bitcoind_a)
    node_manager.wait_for_peers_connections(florestad_node, bitcoind_b)

    info = florestad_node.rpc.get_added_node_info()
    assert len(info) == 2

    added_addresses = {entry["addednode"] for entry in info}
    assert bitcoind_a.p2p_url in added_addresses
    assert bitcoind_b.p2p_url in added_addresses

    # Both should be connected with addresses populated
    for entry in info:
        assert entry["connected"] is True
        assert len(entry["addresses"]) == 1
        assert entry["addresses"][0]["connected"] == "outbound"


@pytest.mark.rpc
def test_get_added_node_info_filter_by_node(
    node_manager, florestad_node, add_node_with_extra_args
):
    """
    The optional node parameter should filter results to a single entry.
    """
    bitcoind_a = add_node_with_extra_args(NodeType.BITCOIND, ["-v2transport=0"])
    bitcoind_b = add_node_with_extra_args(NodeType.BITCOIND, ["-v2transport=0"])

    florestad_node.rpc.addnode(bitcoind_a.p2p_url, "add", v2transport=False)
    florestad_node.rpc.addnode(bitcoind_b.p2p_url, "add", v2transport=False)
    node_manager.wait_for_peers_connections(florestad_node, bitcoind_a)
    node_manager.wait_for_peers_connections(florestad_node, bitcoind_b)

    # Filter for node A only
    filtered = florestad_node.rpc.get_added_node_info(bitcoind_a.p2p_url)
    assert len(filtered) == 1
    assert filtered[0]["addednode"] == bitcoind_a.p2p_url

    # Filter for node B only
    filtered = florestad_node.rpc.get_added_node_info(bitcoind_b.p2p_url)
    assert len(filtered) == 1
    assert filtered[0]["addednode"] == bitcoind_b.p2p_url

    # Filter for non-existent node returns empty list
    filtered_empty = florestad_node.rpc.get_added_node_info("1.2.3.4:9999")
    assert len(filtered_empty) == 0


@pytest.mark.rpc
def test_get_added_node_info_after_remove(
    node_manager, florestad_node, add_node_with_extra_args
):
    """
    After removing a manually added node, it should no longer appear in getaddednodeinfo.
    """
    bitcoind = add_node_with_extra_args(NodeType.BITCOIND, ["-v2transport=0"])

    florestad_node.rpc.addnode(bitcoind.p2p_url, "add", v2transport=False)
    node_manager.wait_for_peers_connections(florestad_node, bitcoind)

    info = florestad_node.rpc.get_added_node_info()
    assert len(info) == 1

    florestad_node.rpc.addnode(bitcoind.p2p_url, "remove")

    info = florestad_node.rpc.get_added_node_info()
    assert len(info) == 0


@pytest.mark.rpc
def test_get_added_node_info_connect_flag_not_listed(
    node_manager, add_node_with_extra_args
):
    """
    Peers added via --connect should NOT appear in getaddednodeinfo.
    Only peers added via the addnode RPC are listed.
    """
    bitcoind = add_node_with_extra_args(NodeType.BITCOIND, ["-v2transport=0"])
    florestad = add_node_with_extra_args(
        NodeType.FLORESTAD,
        [f"--connect={bitcoind.p2p_url}"],
    )

    node_manager.wait_for_peers_connections(florestad, bitcoind)

    # The --connect peer must NOT appear in getaddednodeinfo
    info = florestad.rpc.get_added_node_info()
    assert isinstance(info, list)
    assert len(info) == 0


@pytest.mark.rpc
def test_get_added_node_info_invalid_node_filter(florestad_node):
    """
    Passing an invalid address as the node filter should return an error.
    """
    with pytest.raises(HTTPError):
        florestad_node.rpc.get_added_node_info("not-a-valid-address")
