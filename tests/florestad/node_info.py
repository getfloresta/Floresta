"""
Tests for node information exchanged between Floresta and other peers.
"""

import re
import pytest


@pytest.mark.rpc
def test_node_info(florestad_bitcoind):
    """
    Tests that the node information (e.g., version and subversion) sent by Floresta to other
    peers is correct.
    """
    florestad, bitcoind = florestad_bitcoind

    peer_info = bitcoind.rpc.get_peerinfo()

    assert len(peer_info) == 1
    assert peer_info[0]["services"] == "0000000001000009"
    assert peer_info[0]["version"] == 70016
    assert re.match(r"\/Floresta:\d+\.\d+\.\d+.*\/", peer_info[0]["subver"])
    assert peer_info[0]["inbound"] is True

    peer_info = florestad.rpc.get_peerinfo()
    assert peer_info[0]["address"] == bitcoind.p2p_url
    assert peer_info[0]["kind"] == "regular"
    assert (
        peer_info[0]["services"]
        == "ServiceFlags(NETWORK|WITNESS|NETWORK_LIMITED|P2P_V2)"
    )
    assert peer_info[0]["transport_protocol"] == "V2"
    assert re.match(r"\/Satoshi:\d*\.\d*\.\d*\/", peer_info[0]["user_agent"])
