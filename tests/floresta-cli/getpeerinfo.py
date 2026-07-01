# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getpeerinfo.py

This functional test cli utility interacts with a Floresta node through `getpeerinfo`.
"""

import pytest

EXPECTED_KEYS = {
    "id",
    "address",
    "services",
    "user_agent",
    "initial_height",
    "kind",
    "state",
    "transport_protocol",
}

EXPECTED_KINDS = {
    "outbound-full-relay",
    "block-relay-only",
    "manual",
    "feeler",
    "addr-fetch",
}


def assert_peer_info_shape(peer):
    """Assert the stable, documented fields returned for a peer."""
    assert set(peer.keys()) == EXPECTED_KEYS
    assert isinstance(peer["id"], int)
    assert isinstance(peer["address"], str)
    assert isinstance(peer["services"], str)
    assert isinstance(peer["user_agent"], str)
    assert isinstance(peer["initial_height"], int)
    assert peer["kind"] in EXPECTED_KINDS
    assert peer["state"] in {"Ready", "Awaiting", "Banned"}
    assert peer["transport_protocol"] in {"V1", "V2"}


@pytest.mark.rpc
def test_peer_info(florestad_node, bitcoind_node, node_manager):
    """
    Test `getpeerinfo` with a fresh node and after an addnode-created peer.
    """

    result = florestad_node.rpc.get_peerinfo()

    assert isinstance(result, list)
    assert len(result) == 0

    node_manager.connect_nodes(florestad_node, bitcoind_node)

    result = florestad_node.rpc.get_peerinfo()

    assert len(result) == 1

    peer = result[0]
    assert_peer_info_shape(peer)
    assert peer["kind"] == "manual"
    assert peer["state"] == "Ready"
    assert "Satoshi" in peer["user_agent"]
