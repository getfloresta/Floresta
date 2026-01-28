"""
addnode-test.py

This functional test cli utility to interact with a Floresta
node with `addnode` that should be both compliant with the Bitcoin-core
in context of the v1 transport protocol.

(see more at https://bitcoincore.org/en/doc/29.0.0/rpc/network/addnode/)
"""

import re
import time

from test_framework import FlorestaTestFramework

DATA_DIR = FlorestaTestFramework.get_integration_test_dir()


class AddnodeTestV1(FlorestaTestFramework):

    def set_test_params(self):
        """
        Setup the two nodes (florestad and bitcoind)
        in the same regtest network.
        """
        name = self.__class__.__name__.lower()
        self.v2transport = False
        self.data_dirs = AddnodeTestV1.create_data_dirs(DATA_DIR, name, 2)

        self.florestad = self.add_node(
            variant="florestad",
            extra_args=[f"--data-dir={self.data_dirs[0]}"],
        )

        self.bitcoind = self.add_node(
            variant="bitcoind",
            extra_args=[
                f"-datadir={self.data_dirs[1]}",
                f"-v2transport={1 if self.v2transport else 0}",
            ],
        )

    def verify_peer_connection_state(self, is_connected: bool):
        """
        Verify whether a peer is connected; if connected, validate the peer details.
        """
        self.log(
            f"Checking if bitcoind is {'connected' if is_connected else 'disconnected'}"
        )
        self.wait_for_peers_connections(self.florestad, self.bitcoind, is_connected)

        expected_peer_count = 1 if is_connected else 0
        peer_info = self.florestad.rpc.get_peerinfo()
        self.assertEqual(len(peer_info), expected_peer_count)

        if is_connected:
            self.assertEqual(peer_info[0]["transport_protocol"], "V1")

        if self.bitcoind.daemon.is_running:
            bitcoin_peers = self.bitcoind.rpc.get_peerinfo()
            self.assertEqual(len(bitcoin_peers), expected_peer_count)

    def floresta_addnode_with_command(self, command: str):
        """
        Send an `addnode` RPC from Floresta to the bitcoind peer using the given command.
        """
        self.log(f"Floresta adding node {self.bitcoind_addr} with command '{command}'")
        result = self.florestad.rpc.addnode(
            node=self.bitcoind_addr,
            command=command,
            v2transport=self.v2transport,
        )

        self.assertIsNone(result)

    def stop_bitcoind(self):
        """
        Stop the bitcoind node.
        """
        self.log(f"Stopping bitcoind node")
        self.bitcoind.stop()
        self.florestad.rpc.ping()

    def run_test(self):
        """
        Tests the addnode functionality for Floresta, verifying that it can establish connections
        based on the command passed (e.g., add, onetry, remove), behaves correctly when a peer
        disconnects according to the connection type, and properly handles adding and removing peers.
        """
        self.log("===== Starting florestad and bitcoind nodes")
        self.run_node(self.florestad)
        self.run_node(self.bitcoind)

        self.bitcoind_addr = f"127.0.0.1:{self.bitcoind.get_port('p2p')}"

        self.log("===== Add bitcoind as a persistent peer to Floresta")
        self.floresta_addnode_with_command("add")
        self.verify_peer_connection_state(is_connected=True)

        self.stop_bitcoind()
        self.verify_peer_connection_state(is_connected=False)

        self.run_node(self.bitcoind)
        self.verify_peer_connection_state(is_connected=True)

        self.log("===== Verify Floresta does not add the same persistent peer twice")
        self.floresta_addnode_with_command("add")
        # This function expects 1 peer connected to florestad
        self.verify_peer_connection_state(is_connected=True)

        self.floresta_addnode_with_command("onetry")
        # This function expects 1 peer connected to florestad
        self.verify_peer_connection_state(is_connected=True)

        self.log("===== Remove bitcoind from Floresta's persistent peer list")
        self.floresta_addnode_with_command("remove")
        self.verify_peer_connection_state(is_connected=True)

        self.stop_bitcoind()
        self.verify_peer_connection_state(is_connected=False)

        self.run_node(self.bitcoind)
        self.verify_peer_connection_state(is_connected=False)

        self.log(
            "===== Add bitcoind as a one-time (onetry) connection; expect a single connection"
        )
        self.floresta_addnode_with_command("onetry")
        self.verify_peer_connection_state(is_connected=True)

        self.stop_bitcoind()
        self.verify_peer_connection_state(is_connected=False)

        self.run_node(self.bitcoind)
        self.verify_peer_connection_state(is_connected=False)


if __name__ == "__main__":
    AddnodeTestV1().main()
