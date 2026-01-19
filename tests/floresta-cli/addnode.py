"""Tests for florestad addnode RPC behavior."""

import time
import pytest


@pytest.mark.rpc
def test_add_node_v1(
    setup_logging, node_manager, florestad_node, add_node_with_extra_args
):
    """Test addnode behavior using v1 transport."""
    log = setup_logging
    florestad = florestad_node
    bitcoind = add_node_with_extra_args("bitcoind", ["-v2transport=0"])
    is_v2 = False

    test_node = TestAddNode(log, node_manager, florestad, bitcoind, is_v2)
    test_node.run_test()


@pytest.mark.rpc
def test_add_node_v2(
    setup_logging, node_manager, florestad_node, add_node_with_extra_args
):
    """Test addnode behavior using v2 transport."""
    log = setup_logging
    florestad = florestad_node
    bitcoind = add_node_with_extra_args("bitcoind", ["-v2transport=1"])
    is_v2 = True

    test_node = TestAddNode(log, node_manager, florestad, bitcoind, is_v2)
    test_node.run_test()


class TestAddNode:
    """Test cases for adding and managing bitcoind peers in florestad."""

    # pylint: disable=too-many-arguments, too-many-positional-arguments
    def __init__(self, log, node_manager, florestad, bitcoind, is_v2):
        """Initialize attributes used by test methods to satisfy linters."""
        self.log = log
        self.node_manager = node_manager
        self.florestad = florestad
        self.bitcoind = bitcoind
        self.is_v2 = is_v2
        self.bitcoind_addr = None

    def verify_peer_connection(self, is_connected: bool):
        """
        Verify whether a peer is connected; if connected, validate the peer details.
        """
        expected_number_peer = 1 if is_connected else 0
        peer_info = []
        deadline = time.time() + 15
        while time.time() < deadline:
            peer_info = self.florestad.rpc.get_peerinfo()
            if len(peer_info) == expected_number_peer:
                break
            time.sleep(1)
            self.florestad.rpc.ping()

        assert len(peer_info) == expected_number_peer

        if not is_connected:
            return

        for peer in peer_info:
            assert peer["state"] == "Ready"
            assert peer["address"] == self.bitcoind_addr
            assert "Satoshi" in peer["user_agent"]

        bitcoin_peer_info = self.bitcoind.rpc.get_peerinfo()
        assert len(bitcoin_peer_info) == 1
        assert "Floresta" in bitcoin_peer_info[0]["subver"]

    def stop_bitcoind(self):
        """Stop the bitcoind node and ensure florestad detects the disconnection."""
        self.bitcoind.stop()
        self.florestad.rpc.ping()
        self.verify_peer_connection(False)

    def run_test(self):
        """Main test workflow for addnode behavior."""
        self.bitcoind_addr = f"127.0.0.1:{self.bitcoind.get_port('p2p')}"

        self.log.info(
            "Adding bitcoind (%s) to florestad (v2transport=%s)...",
            self.bitcoind_addr,
            self.is_v2,
        )
        # intentionally ignore return value
        _result = self.florestad.rpc.addnode(
            node=self.bitcoind_addr, command="add", v2transport=self.is_v2
        )

        self.log.info(
            "Waiting for florestad to establish P2P connection with "
            "bitcoind at %s...",
            self.bitcoind_addr,
        )
        self.verify_peer_connection(True)

        self.log.info(
            "Stopping bitcoind to check if florestad detects the disconnection..."
        )
        self.stop_bitcoind()

        self.log.info(
            "Restarting bitcoind to test florestad's automatic reconnection..."
        )
        self.node_manager.run_node(self.bitcoind)
        self.verify_peer_connection(True)

        self.log.info(
            "Checking behavior on duplicate addnode attempts "
            "(should not create duplicate entries)..."
        )

        self.log.debug(
            f"Attempting to add node {self.bitcoind_addr} again with command 'add' "
            f"(v2transport={self.is_v2})..."
        )
        self.florestad.rpc.addnode(
            node=self.bitcoind_addr, command="add", v2transport=self.is_v2
        )
        self.verify_peer_connection(True)

        self.log.debug(
            "Attempting a single connection ('onetry') to node %s "
            "(v2transport=%s)...",
            self.bitcoind_addr,
            self.is_v2,
        )
        self.florestad.rpc.addnode(
            node=self.bitcoind_addr, command="onetry", v2transport=self.is_v2
        )
        self.verify_peer_connection(True)

        self.log.info(
            "Removing node %s from florestad and verifying subsequent state...",
            self.bitcoind_addr,
        )
        self.florestad.rpc.addnode(
            node=self.bitcoind_addr, command="remove", v2transport=self.is_v2
        )
        self.verify_peer_connection(True)
        self.stop_bitcoind()

        self.log.debug(
            "Restarting bitcoind to confirm florestad does not reconnect after "
            "node removal..."
        )
        self.node_manager.run_node(self.bitcoind)
        self.florestad.rpc.ping()
        self.verify_peer_connection(False)

        self.log.info("Testing 'onetry' command for a single connection to bitcoind...")
        self.florestad.rpc.addnode(
            node=self.bitcoind_addr, command="onetry", v2transport=self.is_v2
        )
        self.verify_peer_connection(True)

        self.stop_bitcoind()
        self.verify_peer_connection(False)

        self.log.debug(
            "Restarting bitcoind again to ensure there is no persistent "
            "reconnection after 'onetry'..."
        )
        self.node_manager.run_node(self.bitcoind)
        self.florestad.rpc.ping()
        self.verify_peer_connection(False)
