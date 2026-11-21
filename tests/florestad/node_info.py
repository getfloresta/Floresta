# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Tests for node information exchanged between Floresta and other peers.
"""

import re
import pytest
from test_framework.util import assert_bitcoind_service_fields


@pytest.mark.rpc
def test_node_info(florestad_bitcoind):
    """
    Tests that the node information (e.g., version and subversion) sent by Floresta to other
    peers is correct.
    """
    florestad, bitcoind = florestad_bitcoind

    peers_seen_by_bitcoind = bitcoind.rpc.get_peerinfo()

    assert len(peers_seen_by_bitcoind) == 1
    floresta_peer = peers_seen_by_bitcoind[0]
    assert floresta_peer["services"] == "0000000000001808"  # WITNESS | P2P_V2 | UTREEXO
    assert floresta_peer["version"] == 70016
    assert re.match(r"\/Floresta:\d+\.\d+\.\d+.*\/", floresta_peer["subver"])
    assert floresta_peer["inbound"] is True

    peers_seen_by_floresta = florestad.rpc.get_peerinfo()
    bitcoind_peer = peers_seen_by_floresta[0]
    assert bitcoind_peer["addr"] == bitcoind.p2p_url
    assert bitcoind_peer["connection_type"] == "manual"
    assert_bitcoind_service_fields(bitcoind_peer)
    assert bitcoind_peer["transport_protocol_type"] == "v2"
    assert re.match(r"\/Satoshi:\d*\.\d*\.\d*\/", bitcoind_peer["subver"])
