"""
p2p_resilience.py

Test Floresta's resilience against DoS attacks via P2P protocol.

This test module verifies that the Floresta node properly defends against
Denial of Service (DoS) attacks by validating its peer rejection and disconnection
mechanisms across both v1 and v2 P2P protocol versions.
"""

from random import sample
import time
import pytest

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
from test_framework.util import wait_until
from test_framework.p2p import P2PInterface
from test_framework.messages import MAX_PROTOCOL_MESSAGE_LENGTH, MAX_MSG_PER_SECOND


class TestP2pResilience:
    """
    Test Floresta's resilience against DoS attacks via P2P protocol.

    This test class validates the Floresta node's defense mechanisms against
    various Denial of Service (DoS) attack vectors on the P2P interface,
    ensuring the node properly rejects malicious peers across both v1 and v2
    P2P protocol versions.
    """

    # Define attributes at class level to avoid "defined outside __init__" warnings
    florestad = None
    log = None
    node_manager = None
    p2p_conn_v1 = None
    p2p_conn_v2 = None

    @pytest.mark.p2p
    def test_oversized_messages_disconnection(
        self, setup_logging, node_manager, florestad_node
    ):
        """
        Test oversized message rejection and peer disconnection.

        Verifies that the Floresta node disconnects from peers sending messages
        larger than the protocol maximum. Tests multiple message types across
        both v1 (MAX_PROTOCOL_MESSAGE_LENGTH) and v2 (16 MiB) limits.
        """
        self.florestad = florestad_node
        self.log = setup_logging
        self.node_manager = node_manager

        message_classes = [
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

        # Limit to 8 random message types for performance reasons.
        # Testing all supported message types would cause excessive test duration.
        # Each message type requires two P2P connections (v1 and v2) and a spam cycle,
        # so testing all ~37 types would be impractical for CI/CD pipelines.
        selected = sample(message_classes, k=min(8, len(message_classes)))
        self.log.info(f"Randomly selected messages: {[m.msgtype for m in selected]}")

        for msg_class in selected:
            self.log.info(f"Testing {msg_class.__name__}")
            self.connect_p2p()

            self.log.info(f"Testing {msg_class.__name__} with version v1")
            msg = self.node_manager.create_msg_random(
                msgtype=msg_class.msgtype, size=MAX_PROTOCOL_MESSAGE_LENGTH + 1
            )
            self.p2p_conn_v1.send_without_ping(msg)
            self.check_disconnection(self.p2p_conn_v1, expected_peer_count=1)

            self.log.info(f"Testing {msg_class.__name__} with version v2")
            msg = self.node_manager.create_msg_random(
                msgtype=msg_class.msgtype, size=16777216 + 1
            )  # 16 MiB + 1 byte
            self.p2p_conn_v2.send_without_ping(msg)
            self.check_disconnection(self.p2p_conn_v2)

    @pytest.mark.p2p
    def test_spam_rate_limiting(self, setup_logging, node_manager, florestad_node):
        """
        Test message spam protection and rate limit enforcement.

        Verifies that the Floresta node enforces rate limiting to reject peers
        sending excessive message volumes. Tests multiple message types across
        both v1 and v2 P2P protocol versions.
        """
        self.florestad = florestad_node
        self.log = setup_logging
        self.node_manager = node_manager

        message_classes = [
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

        # Limit to 2 random message types for performance reasons.
        # Testing all supported message types would cause excessive test duration.
        # Each message type requires two P2P connections (v1 and v2) and a spam cycle,
        # so testing all ~37 types would be impractical for CI/CD pipelines.
        selected = sample(message_classes, k=min(2, len(message_classes)))
        self.log.info(f"Randomly selected messages: {[m.msgtype for m in selected]}")

        for msg in selected:
            msg_type = msg.msgtype
            self.log.info(f"Testing {msg_type}")
            self.connect_p2p()

            self.log.info(f"Testing {msg_type} - small-message spam on v1")
            self.send_spam_p2p_messages(msg=msg, p2p_conn=self.p2p_conn_v1)
            self.check_disconnection(self.p2p_conn_v1, expected_peer_count=1)

            self.log.info(f"Testing {msg_type} - small-message spam on v2")
            self.send_spam_p2p_messages(msg=msg, p2p_conn=self.p2p_conn_v2)
            self.check_disconnection(self.p2p_conn_v2)

    def connect_p2p(self):
        """
        Establish P2P connections to the Floresta node.

        Creates two connections: one using v1 P2P protocol and one using v2.
        Both connections are validated to ensure the node recognizes them.
        """
        self.log.info("Connecting to interface P2P...")
        self.log.info("Connecting with v1 P2P protocol...")
        self.p2p_conn_v1 = self.node_manager.add_p2p_connection_default(
            node=self.florestad, p2p_idx=0, supports_v2_p2p=False
        )
        wait_until(
            predicate=lambda: self.floresta_has_peer_count(expected_peer_count=1),
            error_msg="Floresta node did not connect as expected",
        )

        self.log.info("Connecting with v2 P2P protocol...")
        self.p2p_conn_v2 = self.node_manager.add_p2p_connection_default(
            node=self.florestad, p2p_idx=1, supports_v2_p2p=True
        )
        wait_until(
            predicate=lambda: self.floresta_has_peer_count(expected_peer_count=2),
            error_msg="Floresta node did not connect as expected",
        )

    def floresta_has_peer_count(self, expected_peer_count: int = 0) -> bool:
        """
        Check if the Floresta node has the expected number of peers.
        """
        self.florestad.rpc.ping()
        return len(self.florestad.rpc.get_peerinfo()) == expected_peer_count

    def check_disconnection(self, p2p, expected_peer_count: int = 0):
        """
        Verify peer disconnection after DoS attack.

        Waits for the P2P connection to close and validates that the Floresta
        node properly updated its peer count after disconnection.
        """
        self.log.info("Checking disconnection...")
        p2p.wait_for_disconnect()
        wait_until(
            predicate=lambda: self.floresta_has_peer_count(
                expected_peer_count=expected_peer_count
            ),
            error_msg="Floresta node did not disconnect as expected",
        )

    def send_spam_p2p_messages(
        self,
        p2p_conn: P2PInterface,
        msg,
        message_count: int = MAX_MSG_PER_SECOND * 50,
        check_disconnection: bool = True,
    ):
        """
        Flood a P2P connection with excessive messages to trigger rate limiting.

        Sends a large batch of messages in rapid succession to test the node's
        rate limiting and spam protection mechanisms. Verifies that the node
        disconnects the peer when rate limits are exceeded.
        """
        start = time.time()

        try:
            # Build all messages first to avoid timing issues with message construction during
            # sending.
            messages = [p2p_conn.build_message(msg) for _ in range(message_count)]
            elapsed = time.time() - start
            self.log.debug(f"Built messages in: {elapsed:.2f}s")
            start = time.time()
            for message in messages:
                p2p_conn.send_raw_message(message)

        except IOError as e:
            self.log.debug(f"IOError during message sending: {e}")

        elapsed = time.time() - start
        self.log.debug(f"Sent messages in: {elapsed:.2f}s")

        if check_disconnection:
            if p2p_conn.is_connected:
                time.sleep(1)  # Allow time for potential disconnection to occur
                assert (
                    not p2p_conn.is_connected
                ), "Connection should have closed but is still open"
            self.log.debug("Connection closed during spam (as expected)")
        elif not p2p_conn.is_connected:
            raise RuntimeError("Connection closed unexpectedly")
