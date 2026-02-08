"""
disconnectnode.py

Functional test for the `disconnectnode` RPC.

See the RPC documentation at https://bitcoincore.org/en/doc/29.0.0/rpc/network/disconnectnode/
"""

from time import sleep
from requests.exceptions import HTTPError
from typing import Optional

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class DisconnectNodeTest(FlorestaTestFramework):
    def set_test_params(self):
        """
        Setup `bitcoind` and `florestad` in the same regtest network.
        """

        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)
        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def check_peer_connection_state(self, is_connected: bool):
        """
        Check a peer's connection status to `florestad`.
        """
        self.log(
            f"Checking if bitcoind is {'connected' if is_connected else 'disconnected'}"
        )
        self.wait_for_peers_connections(self.bitcoind, self.florestad, is_connected)

        expected_peer_count = 1 if is_connected else 0
        florestad_peer_info = self.florestad.rpc.get_peerinfo()
        self.assertEqual(len(florestad_peer_info), expected_peer_count)

        if is_connected:
            self.assertEqual(len(florestad_peer_info), 1)

        if self.bitcoind.daemon.is_running:
            bitcoin_peers = self.bitcoind.rpc.get_peerinfo()
            self.assertEqual(len(bitcoin_peers), expected_peer_count)

    def floresta_cli_addnode(self):
        """
        Call the `addnode` RPC from `florestad`.
        """
        self.log(f"florestad: addnode {self.bitcoind.p2p_url} add")
        self.connect_nodes(self.florestad, self.bitcoind)

    def floresta_cli_disconnectnode(
        self, node_address: str = "", node_id: int | None = None
    ):
        """
        Call the `disconnectnode` RPC from `florestad`.
        """
        if node_id is not None:
            self.log(f'florestad: disconnectnode "{node_address}" {node_id}')
            return self.florestad.rpc.disconnectnode(
                node_address=node_address, node_id=node_id
            )
        else:
            self.log(f"florestad: disconnectnode {node_address}")
            return self.florestad.rpc.disconnectnode(
                node_address=node_address,
            )

    def run_test(self):
        """
        Run the `disconnectnode` test.

        Verifies that the RPC fails when called with invalid
        arguments and successfully disconnects from existing peers.
        """

        self.log("===== Starting bitcoind and florestad =====")
        self.run_node(self.bitcoind)
        self.run_node(self.florestad)

        self.log("===== Adding bitcoind as a peer =====")
        self.floresta_cli_addnode()
        self.check_peer_connection_state(is_connected=True)

        self.log("===== Attempting to remove the peer with an invalid node_id =====")
        # Since we only have one peer, it MUST have a `node_id` of 0.
        node_address = ""
        node_id = 1
        with self.assertRaises(HTTPError):
            self.floresta_cli_disconnectnode(node_address, node_id)
        self.check_peer_connection_state(is_connected=True)

        self.log(
            "===== Attempting to disconnect the peer with an invalid node_address (wrong port) ====="
        )
        bitcoind_array = self.bitcoind.p2p_url.split(":")
        bitcoind_ip: str = bitcoind_array[0]
        bitcoind_port: int = int(bitcoind_array[1])
        # Call `disconnectnode` with an invalid `node_address` (wrong port).
        node_address = f"{bitcoind_ip}:{bitcoind_port + 1}"
        with self.assertRaises(HTTPError):
            self.floresta_cli_disconnectnode(node_address)
        self.check_peer_connection_state(is_connected=True)

        self.log(
            "===== Attempting to disconnect the peer with an invalid node_address (wrong IP address) ====="
        )
        # Call `disconnectnode` with an invalid `node_address` (wrong IP address: 127.0.0.2).
        node_address = f"127.0.0.2:{bitcoind_port}"
        with self.assertRaises(HTTPError):
            self.floresta_cli_disconnectnode(node_address)
        self.check_peer_connection_state(is_connected=True)

        self.log(
            "===== Attempting to disconnect the peer with an invalid node_address (malformed address) ====="
        )
        # Call `disconnectnode` with an invalid `node_address` (wrong IP address).
        node_address = f"127.0.0:{bitcoind_port}"
        with self.assertRaises(HTTPError):
            self.floresta_cli_disconnectnode(node_address)
        self.check_peer_connection_state(is_connected=True)

        self.log(
            "===== Attempting to disconnect the peer with a valid node_address ====="
        )
        # Call `disconnectnode` with a valid `node_address`.
        res = self.floresta_cli_disconnectnode(self.bitcoind.p2p_url)
        self.assertIsNone(res)
        self.check_peer_connection_state(is_connected=False)

        # Connect to `bitcoind` again with retry logic.
        self.floresta_cli_addnode()
        max_retries = 20
        for retry in range(max_retries):
            try:
                self.check_peer_connection_state(is_connected=True)
                break
            except AssertionError as e:
                if retry == max_retries - 1:
                    self.log(f"Failed to reconnect after {max_retries} attempts")
                    raise
                sleep(1)

        self.log("===== Attempting to disconnect the peer with a valid node_id =====")
        # Call `disconnectnode` with a valid `node_id`)
        node_id = self.florestad.rpc.get_peerinfo()[0]["id"]
        res = self.floresta_cli_disconnectnode(node_id=node_id)
        self.assertIsNone(res)


if __name__ == "__main__":
    DisconnectNodeTest().main()
