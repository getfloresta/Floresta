# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getpeerinfo.py

This functional test cli utility interacts with a Floresta node through `getpeerinfo`.
"""

import pytest
from test_framework.util import (
    assert_bitcoind_service_fields,
    compare_fields,
    wait_until,
)

EXPECTED_KEYS = {
    "id",
    "addr",
    "services",
    "servicesnames",
    "subver",
    "initial_height",
    "connection_type",
    "state",
    "transport_protocol_type",
    "relaytxes",
    "inbound",
    "bip152_hb_to",
    "bip152_hb_from",
    "timeoffset",
    "permissions",
}

EXPECTED_KINDS = {
    "outbound-full-relay",
    "block-relay-only",
    "manual",
    "feeler",
    "addr-fetch",
}

EXPECTED_PEER_INFO = {
    "relaytxes": False,
    "inbound": False,
    "bip152_hb_to": False,
    "bip152_hb_from": False,
    "permissions": [],
    "connection_type": "manual",
    "state": "Ready",
}


def assert_peer_info_shape(peer):
    """Assert the stable, documented fields returned for a peer."""
    assert set(peer.keys()) == EXPECTED_KEYS
    assert isinstance(peer["id"], int)
    assert isinstance(peer["addr"], str)
    assert isinstance(peer["services"], str)
    assert isinstance(peer["servicesnames"], list)
    assert isinstance(peer["subver"], str)
    assert isinstance(peer["initial_height"], int)
    assert peer["connection_type"] in EXPECTED_KINDS
    assert peer["state"] in {"Ready", "Awaiting", "Banned"}
    assert peer["transport_protocol_type"] in {"v1", "v2"}
    assert isinstance(peer["relaytxes"], bool)
    assert isinstance(peer["inbound"], bool)
    assert isinstance(peer["bip152_hb_to"], bool)
    assert isinstance(peer["bip152_hb_from"], bool)
    assert isinstance(peer["timeoffset"], int)
    assert isinstance(peer["permissions"], list)


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
    assert isinstance(result, list)
    assert len(result) == 1

    peer = result[0]
    assert_peer_info_shape(peer)
    assert_bitcoind_service_fields(peer)
    compare_fields(peer, EXPECTED_PEER_INFO)
    assert abs(peer["timeoffset"]) < 60
    assert "Satoshi" in peer["subver"]


def has_ready_peer(node):
    """Whether `node` has exactly one peer that finished the handshake."""
    peers = node.rpc.get_peerinfo()
    return len(peers) == 1 and peers[0]["state"] == "Ready"


@pytest.mark.rpc
def test_peer_info_automatic_peer(florestad_utreexod):
    """
    Test `getpeerinfo` with a peer that wasn't added through `addnode`.

    On shutdown, florestad saves its Utreexo peers as anchors. On startup, it
    reconnects to them as automatic outbound connections, not manual ones.
    """
    florestad, _ = florestad_utreexod

    florestad.stop()
    florestad.start()

    wait_until(lambda: has_ready_peer(florestad), error_msg="Anchor peer not ready")

    peer = florestad.rpc.get_peerinfo()[0]
    assert_peer_info_shape(peer)
    assert peer["connection_type"] == "outbound-full-relay"
