"""
p2p_spam_msg.py

Test that the Floresta node properly handles P2P message spam and rate limiting.

This test verifies that when a node receives an excessive number of messages
from a peer, it properly enforces rate limiting and disconnects the offending
peer. The test floods the node with various P2P message types across both v1
and v2 protocol versions to ensure robustness against spam attacks.
"""

import time

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
    P2PHeaderAndShortIDs,
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
from test_framework.p2p import P2PInterface
from test_framework.util import wait_until
from random import sample


class TestP2pSpamMessages(FlorestaTestFramework):
    """
    Test Floresta's resilience to P2P message spam and rate limiting enforcement.

    This test verifies that the node correctly disconnects peers that exceed
    the maximum message rate (MAX_MSG_PER_SECOND). It floods the node with
    all supported message types across both v1 and v2 P2P protocol versions
    to ensure protection against spam attacks and DoS vulnerabilities.
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
        Run the P2P message spam test.

        For each P2P protocol version (v1 and v2), iterates through all
        supported message types, sends excessive quantities of each message
        to the node, and verifies that the node disconnects the peer due to
        rate limit violations.
        """
        self.run_node(self.florestad)

        self.message_classes = [
            msg_version(),
            msg_verack(),
            msg_addr(),
            msg_addrv2(),
            msg_sendaddrv2(),
            msg_inv(),
            msg_getdata(),
            msg_getblocks(),
            msg_tx(),
            msg_wtxidrelay(),
            msg_block(),
            msg_no_witness_block(),
            msg_getaddr(),
            msg_ping(),
            msg_pong(),
            msg_mempool(),
            msg_notfound(),
            msg_sendheaders(),
            msg_getheaders(),
            msg_headers(),
            msg_merkleblock(),
            msg_filterload(),
            msg_filteradd(data=b"\x00" * 32),
            msg_filterclear(),
            msg_feefilter(),
            msg_sendcmpct(),
            msg_cmpctblock(P2PHeaderAndShortIDs()),
            msg_getblocktxn(),
            msg_blocktxn(),
            msg_no_witness_blocktxn(),
            msg_getcfilters(),
            msg_cfilter(),
            msg_getcfheaders(),
            msg_cfheaders(),
            msg_getcfcheckpt(),
            msg_cfcheckpt(),
            msg_sendtxrcncl(),
        ]

        # Limit to 8 random message types for performance reasons.
        # Testing all supported message types would cause excessive test duration.
        # Each message type requires two P2P connections (v1 and v2) and a spam cycle,
        # so testing all ~37 types would be impractical for CI/CD pipelines.
        selected = sample(self.message_classes, k=min(8, len(self.message_classes)))
        self.log(f"Randomly selected messages: {[m.msgtype for m in selected]}")

        for msg in selected:
            msg_type = msg.msgtype
            self.log(f"Testing {msg_type}")

            self.connect_p2p(supports_v2_p2p=False)
            self.log(f"Testing {msg_type} - small-message spam on v1")
            self.send_spam_p2p_messages(msg=msg, p2p_conn=self.p2p_conn)
            self.check_disconnection(self.p2p_conn)

            self.connect_p2p(supports_v2_p2p=True)
            self.log(f"Testing {msg_type} - small-message spam on v2")
            self.send_spam_p2p_messages(msg=msg, p2p_conn=self.p2p_conn)
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
    TestP2pSpamMessages().main()
