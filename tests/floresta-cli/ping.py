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
    assert "ping" not in peer_info[0]["bytesrecv_per_msg"]

    time.sleep(1)
    florestad.rpc.ping()

    peer_info = bitcoind.rpc.get_peerinfo()
    assert "ping" in peer_info[0]["bytesrecv_per_msg"]
