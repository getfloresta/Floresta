"""
p2p_addr_relay.py

Test suite for P2P address relay functionality in Floresta.
Verifies that the node correctly handles address messages (addr and addrv2),
enforces message size limits, and responds to getaddr requests appropriately.
"""

import time
import random

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.messages import msg_addrv2, msg_sendaddrv2, msg_getaddr
from test_framework.p2p import (
    P2PInterface,
    P2P_SERVICES,
)
from test_framework.util import wait_until


class AddrReceiver(P2PInterface):
    """
    A custom P2P interface that listens for and validates address messages.
    Tracks whether addr (v1) and addrv2 messages are received.
    """

    addr_received_and_checked = False
    addrv2_received_and_checked = False

    def __init__(self, support_addrv2=True):
        super().__init__(support_addrv2=support_addrv2)

    def on_addr(self, message):
        # Floresta does not send peer addresses to other nodes
        if len(message.addrs) == 0:
            self.addr_received_and_checked = True

    def on_addrv2(self, message):
        # Floresta does not send peer addresses to other nodes
        if len(message.addrs) == 0:
            self.addrv2_received_and_checked = True

    def wait_for_addr(self):
        """Wait for an addr message to be received."""
        self.wait_until(lambda: "addr" in self.last_message)

    def wait_for_addrv2(self):
        """Wait for an addrv2 message to be received."""
        self.wait_until(lambda: "addrv2" in self.last_message)


class TestP2pAddrRelay(FlorestaTestFramework):
    """
    Test that Floresta returns addresses when receiving GetAddr.
    """

    def set_test_params(self):
        """
        Here we define setup for test
        """

        self.florestad = self.add_node_default_args(
            variant=NodeType.FLORESTAD,
        )

    def run_test(self):
        """
        Execute the address relay tests.
        """

        self.run_node(self.florestad)
        self.default_msg = msg_addrv2()
        self.default_msg.addrs = self.create_node_address(10)

        self.log("Testing sendaddrv2 message after handshake")
        self.connect_p2p()

        self.p2p_conn.send_without_ping(msg_sendaddrv2())
        self.check_disconnection(self.p2p_conn)

        self.log("Testing addrv2 message ")
        self.connect_p2p()
        self.p2p_conn.send_and_ping(self.default_msg)
        assert self.p2p_conn.is_connected
        assert self.floresta_has_peer_count(expected_peer_count=1)

        self.log(
            "Testing addrv2 message with varying number of addresses to check for disconnection on oversized messages"
        )
        msg_oversized = msg_addrv2()

        for quantity in range(998, 1002):
            addr = self.create_node_address(quantity)
            msg_oversized.addrs = addr
            msg_size = self.calc_addrv2_msg_size(addr)
            self.log(
                f"Testing addrv2 message with {len(msg_oversized.addrs)} addresses (size: {msg_size} bytes)"
            )
            if quantity > 1000:
                self.p2p_conn.send_without_ping(msg_oversized)
                self.check_disconnection(self.p2p_conn)
            else:
                self.p2p_conn.send_and_ping(msg_oversized)
                assert (
                    self.p2p_conn.is_connected
                ), f"Node should still be connected after sending addrv2 message with {len(msg_oversized.addrs)} addresses"

        self.log(
            "Node disconnected as expected after sending an oversized addrv2 message"
        )
        assert (
            not self.p2p_conn.is_connected
        ), "p2p_default should be disconnected after sending an oversized addrv2 message"
        assert (
            len(self.florestad.rpc.get_peerinfo()) == 0
        ), "Floresta node should have no peers connected"

        self.log("Testing getaddr message")
        self.p2p_receiver_v2 = self.add_p2p_connection(
            node=self.florestad, p2p_idx=0, p2p_conn=AddrReceiver()
        )
        self.p2p_receiver_v2.send_without_ping(msg_getaddr())
        self.p2p_receiver_v2.wait_for_addrv2()
        assert self.p2p_receiver_v2.addrv2_received_and_checked

        self.p2p_receiver_v1 = self.add_p2p_connection(
            node=self.florestad,
            p2p_idx=1,
            p2p_conn=AddrReceiver(support_addrv2=False),
        )
        self.p2p_receiver_v1.send_without_ping(msg_getaddr())
        self.p2p_receiver_v1.wait_for_addr()
        assert self.p2p_receiver_v1.addr_received_and_checked

    def connect_p2p(self, expected_peer_count: int = 1):
        """Establish a P2P connection to the Floresta node."""
        self.log("Connecting to interface P2P...")
        self.p2p_conn = self.add_p2p_connection_default(
            node=self.florestad,
            p2p_idx=0,
        )
        wait_until(
            predicate=lambda: self.floresta_has_peer_count(
                expected_peer_count=expected_peer_count
            ),
            error_msg="Floresta node did not connect as expected",
        )

    def floresta_has_peer_count(self, expected_peer_count: int = 0) -> bool:
        """Check if the Floresta node has the expected number of peers."""
        self.florestad.rpc.ping()
        return len(self.florestad.rpc.get_peerinfo()) == expected_peer_count

    def check_disconnection(self, p2p, expected_peer_count: int = 0):
        """Check if the Floresta node has the expected number of peers after disconnection."""
        self.log("Checking disconnection...")
        p2p.wait_for_disconnect()
        wait_until(
            predicate=lambda: self.floresta_has_peer_count(
                expected_peer_count=expected_peer_count
            ),
            error_msg="Floresta node did not disconnect as expected",
        )

    def calc_addrv2_msg_size(self, addrs):
        """Calculate the serialized size of an addrv2 message in bytes."""
        size = 1  # vector length byte
        for addr in addrs:
            size += 4  # time
            size += 1  # services, COMPACTSIZE(P2P_SERVICES)
            size += 1  # network id
            size += 1  # address length byte
            size += addr.ADDRV2_ADDRESS_LENGTH[addr.net]  # address
            size += 2  # port

        return size


if __name__ == "__main__":
    TestP2pAddrRelay().main()
