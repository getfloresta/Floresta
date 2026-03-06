"""Tests for florestad addnode RPC behavior."""

import pytest

from test_framework.node import NodeType


@pytest.mark.rpc
def test_add_node_v1(
    setup_logging, node_manager, florestad_node, add_node_with_extra_args
):
    """Test addnode behavior using v1 transport."""
    log = setup_logging
    florestad = florestad_node
    bitcoind = add_node_with_extra_args(NodeType.BITCOIND, ["-v2transport=0"])
    is_v2 = False

    test_node = AddNodeTest(log, node_manager, florestad, bitcoind, is_v2)
    test_node.run_test()


@pytest.mark.rpc
def test_add_node_v2(
    setup_logging, node_manager, florestad_node, add_node_with_extra_args
):
    """Test addnode behavior using v2 transport."""
    log = setup_logging
    florestad = florestad_node
    bitcoind = add_node_with_extra_args(NodeType.BITCOIND, ["-v2transport=1"])
    is_v2 = True

    test_node = AddNodeTest(log, node_manager, florestad, bitcoind, is_v2)
    test_node.run_test()


class AddNodeTest:
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

    def verify_peer_connection_state(self, is_connected: bool):
        """
        Verify whether a peer is connected; if connected, validate the peer details.
        """
        self.log.info(
            f"Checking if bitcoind is {'connected' if is_connected else 'disconnected'}"
        )
        self.node_manager.wait_for_peers_connections(
            self.florestad, self.bitcoind, is_connected
        )

        expected_peer_count = 1 if is_connected else 0
        peer_info = self.florestad.rpc.get_peerinfo()
        assert len(peer_info) == expected_peer_count

        if is_connected:
            assert peer_info[0]["transport_protocol"] == ("V2" if self.is_v2 else "V1")

        if self.bitcoind.daemon.is_running:
            bitcoin_peers = self.bitcoind.rpc.get_peerinfo()
            assert len(bitcoin_peers) == expected_peer_count

    def floresta_addnode_with_command(self, command: str):
        """
        Send an `addnode` RPC from Floresta to the bitcoind peer using the given command.
        """
        self.log.info(
            f"Floresta adding node {self.bitcoind.p2p_url} with command '{command}'"
        )
        self.node_manager.connect_nodes(
            self.florestad, self.bitcoind, command, self.is_v2
        )

    def stop_bitcoind(self):
        """
        Stop the bitcoind node.
        """
        self.log.info("Stopping bitcoind node")
        self.bitcoind.stop()
        self.florestad.rpc.ping()
        self.verify_peer_connection_state(False)

    def run_test(self):
        """Main test workflow for addnode behavior."""
        self.log.info("===== Add bitcoind as a persistent peer to Floresta")
        self.floresta_addnode_with_command("add")
        self.verify_peer_connection_state(is_connected=True)

        self.stop_bitcoind()
        self.verify_peer_connection_state(is_connected=False)

        self.node_manager.run_node(self.bitcoind)
        self.verify_peer_connection_state(is_connected=True)

        self.log.info(
            "===== Verify Floresta does not add the same persistent peer twice"
        )
        self.floresta_addnode_with_command("add")
        # This function expects 1 peer connected to florestad
        self.verify_peer_connection_state(is_connected=True)

        self.floresta_addnode_with_command("onetry")
        # This function expects 1 peer connected to florestad
        self.verify_peer_connection_state(is_connected=True)

        self.log.info("===== Remove bitcoind from Floresta's persistent peer list")
        self.floresta_addnode_with_command("remove")
        self.verify_peer_connection_state(is_connected=True)

        self.stop_bitcoind()

        self.node_manager.run_node(self.bitcoind)
        self.verify_peer_connection_state(is_connected=False)

        self.log.info(
            "===== Add bitcoind as a one-time (onetry) connection; expect a single connection"
        )
        self.floresta_addnode_with_command("onetry")
        self.verify_peer_connection_state(is_connected=True)

        self.stop_bitcoind()
        self.verify_peer_connection_state(is_connected=False)

        self.node_manager.run_node(self.bitcoind)
        self.verify_peer_connection_state(is_connected=False)
