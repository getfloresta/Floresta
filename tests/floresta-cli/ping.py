"""
A test that creates a florestad and a bitcoind node, and connects them. We then
send a ping to bitcoind and check if bitcoind receives it, by calling
`getpeerinfo` and checking that we've received a ping from floresta.
"""

import time
import pytest


@pytest.mark.rpc
def test_ping(florestad_bitcoind):
    """
    Test pinging between florestad and bitcoind nodes.
    """
    florestad, bitcoind = florestad_bitcoind

    florestad.rpc.ping()

    peer_info = bitcoind.rpc.get_peerinfo()
    quantity_message = peer_info[0]["bytesrecv_per_msg"].get("ping", 0)

    time.sleep(1)
    florestad.rpc.ping()

    peer_info = bitcoind.rpc.get_peerinfo()
    assert peer_info[0]["bytesrecv_per_msg"]["ping"], quantity_message * 2
