"""
disconnectnode.py

Functional test for the `disconnectnode` RPC.

See the RPC documentation at https://bitcoincore.org/en/doc/29.0.0/rpc/network/disconnectnode/
"""

from time import sleep
import pytest
from requests.exceptions import HTTPError


@pytest.mark.rpc
def test_disconnect_node(florestad_bitcoind, node_manager, setup_logging):
    """
    Test the `disconnectnode` RPC.
    """
    florestad, bitcoind = florestad_bitcoind
    test = DisconnectNodeTest(florestad, bitcoind, node_manager, setup_logging)
    test.run_test()


class DisconnectNodeTest:
    """
    Test the `disconnectnode` RPC.
    """

    def __init__(self, florestad, bitcoind, node_manager, log):
        """
        Initialize class.
        """
        self.florestad = florestad
        self.bitcoind = bitcoind
        self.node_manager = node_manager
        self.log = log

    def check_peer_connection_state(self, is_connected: bool):
        """
        Check a peer's connection status to `florestad`.
        """
        self.log.info(
            f"Checking if bitcoind is {'connected' if is_connected else 'disconnected'}"
        )
        self.node_manager.wait_for_peers_connections(
            self.bitcoind, self.florestad, is_connected
        )

        expected_peer_count = 1 if is_connected else 0

        florestad_peer_info = self.florestad.rpc.get_peerinfo()
        assert len(florestad_peer_info) == expected_peer_count

        if self.bitcoind.daemon.is_running:
            bitcoin_peers = self.bitcoind.rpc.get_peerinfo()

            assert len(bitcoin_peers) == expected_peer_count

    def floresta_cli_addnode(self):
        """
        Call the `addnode` RPC from `florestad`.
        """
        self.log.info(f"florestad: addnode {self.bitcoind.p2p_url} add")
        self.node_manager.connect_nodes(self.florestad, self.bitcoind)

    def floresta_cli_disconnectnode(
        self, node_address: str = "", node_id: int | None = None
    ):
        """
        Call the `disconnectnode` RPC from `florestad`.
        """
        if node_id is not None:
            self.log.info(f'florestad: disconnectnode "{node_address}" {node_id}')
            return self.florestad.rpc.disconnectnode(
                node_address=node_address, node_id=node_id
            )

        self.log.info(f"florestad: disconnectnode {node_address}")
        return self.florestad.rpc.disconnectnode(
            node_address=node_address,
        )

    def run_test(self):
        """
        Run the `disconnectnode` test.

        Verifies that the RPC fails when called with invalid
        arguments and successfully disconnects from existing peers.
        """
        self.log.info("===== Adding bitcoind as a peer")
        self.floresta_cli_addnode()
        self.check_peer_connection_state(is_connected=True)

        self.log.info("===== Attempting to remove the peer with an invalid node_id")
        # Since we only have one peer, it MUST have a `node_id` of 0.
        node_address = ""
        node_id = 1
        with pytest.raises(HTTPError):
            self.floresta_cli_disconnectnode(node_address, node_id)
        self.check_peer_connection_state(is_connected=True)

        self.log.info("===== Attempting to disconnect the peer with an wrong port")
        bitcoind_array = self.bitcoind.p2p_url.split(":")
        bitcoind_ip: str = bitcoind_array[0]
        bitcoind_port: int = int(bitcoind_array[1])
        # Call `disconnectnode` with an invalid `node_address` (wrong port).
        node_address = f"{bitcoind_ip}:{bitcoind_port + 1}"
        with pytest.raises(HTTPError):
            self.floresta_cli_disconnectnode(node_address)
        self.check_peer_connection_state(is_connected=True)

        self.log.info(
            "===== Attempting to disconnect the peer with an wrong IP address"
        )
        # Call `disconnectnode` with an invalid `node_address` (wrong IP address: 127.0.0.2).
        node_address = f"127.0.0.2:{bitcoind_port}"
        with pytest.raises(HTTPError):
            self.floresta_cli_disconnectnode(node_address)
        self.check_peer_connection_state(is_connected=True)

        self.log.info(
            "===== Attempting to disconnect the peer with an malformed address"
        )
        # Call `disconnectnode` with an invalid `node_address` (wrong IP address).
        node_address = f"127.0.0:{bitcoind_port}"
        with pytest.raises(HTTPError):
            self.floresta_cli_disconnectnode(node_address)
        self.check_peer_connection_state(is_connected=True)

        self.log.info(
            "===== Attempting to disconnect the peer with a valid node_address"
        )
        # Call `disconnectnode` with a valid `node_address`.
        res = self.floresta_cli_disconnectnode(self.bitcoind.p2p_url)
        assert res is None
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
                    self.log.error(
                        f"Peer connection state check failed on retry {retry + 1}: {e}"
                    )
                    self.log(f"Failed to reconnect after {max_retries} attempts")
                    raise
                sleep(1)

        self.log.info("===== Attempting to disconnect the peer with a valid node_id")
        # Call `disconnectnode` with a valid `node_id`)
        node_id = self.florestad.rpc.get_peerinfo()[0]["id"]
        res = self.floresta_cli_disconnectnode(node_id=node_id)
        assert res is None
