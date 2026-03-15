"""
p2p_oversized_msg.py

Test that the Floresta node properly handles oversized P2P messages.

This test verifies that when a node receives a message larger than the
maximum protocol message length, it disconnects the peer. The test sends
oversized versions of various P2P message types across both v1 and v2
protocol versions to ensure proper validation and rejection.
"""

import time
import random

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.messages import (
    msg_version,
    msg_verack,
    msg_addr,
    msg_addrv2,
    msg_sendaddrv2,
    msg_inv,
    msg_getdata,
    msg_getblocks,
    msg_tx,
    msg_wtxidrelay,
    msg_block,
    msg_no_witness_block,
    msg_getaddr,
    msg_ping,
    msg_pong,
    msg_mempool,
    msg_notfound,
    msg_sendheaders,
    msg_getheaders,
    msg_headers,
    msg_merkleblock,
    msg_filterload,
    msg_filteradd,
    msg_filterclear,
    msg_feefilter,
    msg_sendcmpct,
    msg_cmpctblock,
    msg_getblocktxn,
    msg_blocktxn,
    msg_no_witness_blocktxn,
    msg_getcfilters,
    msg_cfilter,
    msg_getcfheaders,
    msg_cfheaders,
    msg_getcfcheckpt,
    msg_cfcheckpt,
    msg_sendtxrcncl,
)
from test_framework.p2p import (
    P2PInterface,
    P2P_SERVICES,
)
from test_framework.util import wait_until


class TestP2pOversizedMessages(FlorestaTestFramework):
    """
    Test Floresta's handling of oversized P2P messages.

    This test verifies that the node correctly rejects and disconnects from
    peers that send messages exceeding MAX_PROTOCOL_MESSAGE_LENGTH. It tests
    all supported message types across both v1 and v2 P2P protocol versions.
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
        Run the oversized message test.
        """

        self.run_node(self.florestad)
        self.versions = ["v1", "v2"]
        self.message_classes = [
            msg_version,
            msg_verack,
            msg_addr,
            msg_addrv2,
            msg_sendaddrv2,
            msg_inv,
            msg_getdata,
            msg_getblocks,
            msg_tx,
            msg_wtxidrelay,
            msg_block,
            msg_no_witness_block,
            msg_getaddr,
            msg_ping,
            msg_pong,
            msg_mempool,
            msg_notfound,
            msg_sendheaders,
            msg_getheaders,
            msg_headers,
            msg_merkleblock,
            msg_filterload,
            msg_filteradd,
            msg_filterclear,
            msg_feefilter,
            msg_sendcmpct,
            msg_cmpctblock,
            msg_getblocktxn,
            msg_blocktxn,
            msg_no_witness_blocktxn,
            msg_getcfilters,
            msg_cfilter,
            msg_getcfheaders,
            msg_cfheaders,
            msg_getcfcheckpt,
            msg_cfcheckpt,
            msg_sendtxrcncl,
        ]

        for version in self.versions:
            for msg_class in self.message_classes:
                self.log(f"Testing {msg_class.__name__} with version {version}")
                self.connect_p2p(supports_v2_p2p=(version == "v2"))

                msg = self.create_msg_random(msgtype=msg_class.msgtype)
                self.p2p_conn.send_without_ping(msg)

                self.check_disconnection(self.p2p_conn)

    def connect_p2p(self, supports_v2_p2p: bool, expected_peer_count: int = 1):
        """Establish a P2P connection to the Floresta node."""
        self.log("Connecting to interface P2P...")
        self.p2p_conn = self.add_p2p_connection_default(
            node=self.florestad, p2p_idx=0, supports_v2_p2p=supports_v2_p2p
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


if __name__ == "__main__":
    TestP2pOversizedMessages().main()
