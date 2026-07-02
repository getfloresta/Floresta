# SPDX-License-Identifier: MIT OR Apache-2.0

"""
tests/test_framework/__init__.py

Adapted from
https://github.com/bitcoin/bitcoin/blob/master/test/functional/test_framework/test_framework.py

Bitcoin Core's functional tests define a metaclass that checks whether the required
methods are defined or not. Floresta's functional tests will follow this battle tested structure.
The difference is that `florestad` will run under a `cargo run` subprocess, which is defined at
`add_node_settings`.
"""

import os
import re
import sys
import copy
import socket
import shutil
import signal
import contextlib
import subprocess
import time
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Pattern, Tuple, Optional

from test_framework.crypto.pkcs8 import (
    create_pkcs8_private_key,
    create_pkcs8_self_signed_certificate,
)
from test_framework.daemon import ConfigP2P
from test_framework.rpc import ConfigRPC
from test_framework.electrum import ConfigElectrum, ConfigTls
from test_framework.node import Node, NodeType
from test_framework.util import Utility, wait_until
from test_framework.p2p import P2P_SERVICES, P2PInterface, NetworkThread
from test_framework.messages import (
    NODE_P2P_V2,
    CAddress,
    msg_generic,
    MAX_PROTOCOL_MESSAGE_LENGTH,
    MAX_MSG_PER_SECOND,
)


# pylint: disable=too-many-public-methods
class FlorestaTestFramework:
    """
    Base class for a floresta test script. Individual floresta
    test scripts should:

    - subclass FlorestaTestFramework;
    - not override the __init__() method;
    - not override the main() method;
    - implement set_test_params();
    - implement run_test();


    This class provides the foundational structure for writing and executing tests
    that interact with Floresta nodes. including their daemons, RPC interfaces, and
    Electrum clients. It abstracts common operations such as node initialization,
    configuration, startup, shutdown, and assertions. Thus allowing test developers
    to focus on the specific logic of their tests.

    The framework is designed to be extensible and enforces a consistent structure
    for all test scripts. It ensures that nodes are properly managed during the
    lifecycle of a test, including setup, execution, and teardown phases.

    Key Features:
    - Node Management: Simplifies the process of adding, starting, stopping, and
      configuring nodes of different types (e.g., FLORESTAD, UTREEXOD, BITCOIND).
    - Assertions: Provides a set of built-in assertion methods to validate test
      conditions and automatically handle node cleanup on failure.
    - Logging: Includes utilities for structured logging to help debug and
      understand test execution.
    - Port Management: Dynamically allocates random ports for RPC, P2P, and
      Electrum services to avoid conflicts during parallel test runs.
    """

    def __init__(self, logger, test_name: str):
        """
        Sets test framework defaults.

        Do not override this method. Instead, override the set_test_params() method
        """
        self._test_name = test_name
        self._nodes = []
        self._log = logger
        self._p2p_interface = []

    @property
    def test_name(self) -> str:
        """
        Get the test name
        """
        return self._test_name

    @property
    def log(self):
        """Getter for `log` property"""
        return self._log

    def create_data_dir_for_daemon(self, node_type: NodeType) -> str:
        """
        Create a data directory for the daemon to be run.
        """
        tempdir = str(Utility.get_integration_test_dir())
        path_name = node_type.value.lower() + str(
            self.count_nodes_by_variant(node_type)
        )
        datadir = os.path.normpath(
            os.path.join(tempdir, "data", self.test_name, path_name)
        )
        os.makedirs(datadir, exist_ok=True)

        return datadir

    def count_nodes_by_variant(self, variant: NodeType) -> int:
        """
        Count the number of nodes of a given variant.
        """
        return sum(1 for node in self._nodes if node.variant == variant)

    def add_node_default_args(self, variant: NodeType) -> Node:
        """
        Add a node with default configurations.

        This function initializes a node of the specified variant
        (e.g., FLORESTAD, UTREEXOD, BITCOIND) using default RPC, P2P, and
        Electrum configurations.
        """
        return self._add_node_default_config(variant=variant, extra_args=[], tls=False)

    def add_node_with_tls(self, variant: NodeType) -> Node:
        """
        Add a node with default configurations and TLS enabled.

        This function creates a node with default RPC, P2P, and Electrum configurations,
        enabling TLS for the Electrum server.
        """
        return self._add_node_default_config(variant=variant, extra_args=[], tls=True)

    def add_node_extra_args(self, variant: NodeType, extra_args: List[str]) -> Node:
        """
        Add a node with the specified variant and custom extra arguments.

        This function uses default configurations for RPC, P2P, and Electrum,
        and applies the provided extra arguments to the node.
        """
        return self._add_node_default_config(
            variant=variant, extra_args=extra_args, tls=False
        )

    def _add_node_default_config(
        self, variant: NodeType, extra_args: List[str], tls: bool
    ) -> Node:

        tempdir = str(Utility.get_integration_test_dir())
        targetdir = os.path.normpath(os.path.join(tempdir, "binaries"))
        data_dir = self.create_data_dir_for_daemon(variant)

        node = Node.create_node_default_config(
            variant=variant,
            extra_args=extra_args,
            data_dir=data_dir,
            targetdir=targetdir,
            tls=tls,
            log=self.log,
        )

        self._nodes.append(node)

        return node

    # pylint: disable=too-many-arguments too-many-positional-arguments
    def add_node(
        self,
        variant: NodeType,
        rpc_config: ConfigRPC,
        p2p_config: ConfigP2P,
        extra_args: List[str],
        electrum_config: ConfigElectrum,
        tls: bool,
    ) -> Node:
        """
        Add a node configuration to the test framework.

        This function initializes a node of the specified variant
        (e.g., FLORESTAD, UTREEXOD, BITCOIND) with the provided RPC, P2P, and
        Electrum configurations, as well as any additional arguments.
        The node is added to the framework's list of nodes for testing.
        """
        tempdir = str(Utility.get_integration_test_dir())
        targetdir = os.path.normpath(os.path.join(tempdir, "binaries"))
        data_dir = self.create_data_dir_for_daemon(variant)

        node = Node(
            variant=variant,
            rpc_config=rpc_config,
            p2p_config=p2p_config,
            extra_args=extra_args,
            electrum_config=electrum_config,
            targetdir=targetdir,
            data_dir=data_dir,
            tls=tls,
            log=self.log,
        )
        self._nodes.append(node)

        return node

    def get_node(self, index: int) -> Node:
        """
        Given an index, return a node configuration.
        If the node not exists, raise a IndexError exception.
        """
        if index < 0 or index >= len(self._nodes):
            raise IndexError(
                f"Node {index} not found. Please run it with add_node_settings"
            )
        return self._nodes[index]

    def run_node(self, node: Node):
        """
        Start a node and wait for its RPC server to become available.

        Attempts to start the node up to 3 times, checking if the RPC
        connection is established. If the node fails to start, it is
        terminated and retried.
        """
        for _ in range(3):
            try:
                node.start()
                # Mark the node as having static values
                node.static_values = True
                self.log.debug(f"Node '{node.variant}' started")
                return

            # pylint: disable=broad-exception-caught
            except Exception as e:
                node.stop()
                error = e
                if not node.static_values:
                    self.log.debug(
                        f"Node '{node.variant}' failed to start, updating configs"
                    )
                    node.update_configs()

        raise RuntimeError(f"Error starting node '{node.variant}': {error}")

    def stop_node(self, index: int):
        """
        Stop a node given an index on self._tests.
        """
        node = self.get_node(index)
        return node.stop()

    def stop(self):
        """
        Stop all nodes.
        """
        for i in range(len(self._nodes)):
            self.stop_node(i)

        if (
            hasattr(self, "_network_thread")
            and NetworkThread.network_event_loop is not None
            and self._network_thread.is_alive()
        ):
            self._network_thread.close(timeout=10)
            NetworkThread.network_event_loop = None

    def check_connection(self, peer_one: Node, peer_two: Node, is_connected: bool):
        """
        Check if two peers are connected/disconnected to each other.
        """
        peer_one_running = peer_one.daemon.is_running
        peer_two_running = peer_two.daemon.is_running

        if not peer_one_running and not peer_two_running:
            raise AssertionError(
                f"Neither peer is running: {peer_one.variant}, {peer_two.variant}"
            )

        if peer_one_running != peer_two_running and is_connected:
            raise AssertionError(
                f"Cannot check connection state: Only one peer is running. "
                f"Peer one running: {peer_one_running}, Peer two running: {peer_two_running}"
            )

        # Send pings to both peers to trigger a peer state update
        self._send_peer_pings(peer_one, peer_two)

        peer_two_in_peer_one = (
            peer_one.is_peer_connected(peer_two) if peer_one_running else False
        )
        peer_one_in_peer_two = (
            peer_two.is_peer_connected(peer_one) if peer_two_running else False
        )

        return (
            peer_two_in_peer_one == is_connected
            and peer_one_in_peer_two == is_connected
        )

    def wait_for_peers_connections(
        self, peer_one: Node, peer_two: Node, is_connected: bool = True
    ):
        """
        Wait for two peers to connect/disconnect to each other.
        """
        attempts = 0

        def check_peers_connection():
            nonlocal attempts

            if attempts > 10:
                time.sleep(1)

            attempts += 1

            return self.check_connection(peer_one, peer_two, is_connected)

        wait_until(predicate=check_peers_connection)

        self.log.debug(
            f"Peers {peer_one.variant} and {peer_two.variant} are "
            f"{'connected' if is_connected else 'disconnected'}"
        )

    def _send_peer_pings(self, peer_one: Node, peer_two: Node):
        """Send pings to both peers and log connection status."""
        if peer_one.daemon.is_running:
            peer_one.rpc.ping()
            self.log.debug(
                f"Peer one {peer_one.variant} is connected to peer two {peer_two.variant}: "
                f"{peer_one.is_peer_connected(peer_two)}"
            )

        if peer_two.daemon.is_running:
            peer_two.rpc.ping()
            self.log.debug(
                f"Peer two {peer_two.variant} is connected to peer one {peer_one.variant}: "
                f"{peer_two.is_peer_connected(peer_one)}"
            )

    def connect_nodes(
        self,
        peer_one: Node,
        peer_two: Node,
        command: str = "add",
        v2transport: bool = False,
    ):
        """
        Connect two peers to each other and verify their connection state.
        """
        if peer_two.variant == NodeType.FLORESTAD:
            result = peer_two.connect_node(peer_one, command, v2transport=v2transport)
        else:
            result = peer_one.connect_node(peer_two, command, v2transport=v2transport)

        assert result is None

        self.wait_for_peers_connections(peer_one, peer_two)

    def check_sync_nodes(self, is_finished_ibd: bool = True) -> bool:
        """
        Check if all nodes are synced.

        If is_finished_ibd is True, it will check if all florestad nodes have finished the
        initial block download (IBD) process. Otherwise, it will check if all
        nodes are fully synced with the network.
        """
        if not self._nodes:
            raise AssertionError("No nodes to check for synchronization")

        expected_block = self._nodes[0].rpc.get_block_count()
        for node in self._nodes:
            block_count = node.rpc.get_block_count()

            if (
                node.variant == NodeType.FLORESTAD
                and is_finished_ibd
                and node.rpc.get_blockchain_info()["initialblockdownload"]
            ):
                self.log.debug(
                    f"Node '{node.variant}' has not finished IBD. Block count: {block_count}"
                )
                return False

            if block_count != expected_block:
                self.log.debug(
                    f"Node '{node.variant}' is not synced. Block count: {block_count}, "
                    f"expected: {expected_block}"
                )
                return False

        return True

    def wait_for_sync_nodes(self, is_finished_ibd: bool = True):
        """
        Wait for all nodes to be synced.

        If is_finished_ibd is True, it will wait until all florestad nodes have finished the
        initial block download (IBD) process. Otherwise, it will wait until all
        nodes are fully synced with the network.
        """
        wait_until(lambda: self.check_sync_nodes(is_finished_ibd=is_finished_ibd))

        self.log.debug("All nodes are synced")

    def generate_blocks_and_sync(
        self, blocks: int, is_finished_ibd: bool = True, address: None | str = None
    ):
        """Generate blocks and wait for all nodes to sync."""
        mine_node = None
        for node in self._nodes:
            if address is not None and node.variant == NodeType.BITCOIND:
                mine_node = node
            elif address is None and node.variant in (
                NodeType.UTREEXOD,
                NodeType.BITCOIND,
            ):
                mine_node = node

        if mine_node is None:
            raise AssertionError("No suitable node found for mining blocks")

        self.log.info(f"Generating {blocks} blocks with node '{mine_node.variant}'")
        if address is not None:
            mine_node.rpc.generate_block_to_address(blocks, address)
        else:
            mine_node.rpc.generate(blocks)

        self.wait_for_sync_nodes(is_finished_ibd=is_finished_ibd)

    def add_p2p_connection(
        self,
        node: Node,
        p2p_conn,
        *,
        wait_for_verack=True,
        wait_for_disconnect=False,
        p2p_idx,
        connection_type="outbound-full-relay",
        supports_v2_p2p=True,
        advertise_v2_p2p=True,
        method="onetry",
        **kwargs,
    ):
        """Add an outbound p2p connection from node.

        An outbound connection is made from Node -------> P2PConnection
        - if P2PConnection doesn't advertise_v2_p2p, Node sends version message and v1 P2P is
          followed
        - if P2PConnection both supports_v2_p2p and advertise_v2_p2p, Node sends ellswift bytes and
          v2 P2P is followed

        Parameters:
            p2p_conn: The P2PConnection object
            p2p_idx: Index for the connection (must be different for simultaneous peers)
            supports_v2_p2p: whether p2p_conn supports v2 P2P
            advertise_v2_p2p: whether p2p_conn is advertised to support v2 P2P
            connection_type: Type of connection ("outbound-full-relay", "block-relay-only",
             "addr-fetch", "feeler")
        """
        node_peers = node.rpc.get_connectioncount()

        if NetworkThread.network_event_loop is None:
            network_thread = NetworkThread()
            network_thread.start()
            # pylint: disable=attribute-defined-outside-init
            self._network_thread = network_thread

        def add_connection_callback(address, port):
            self.log.debug(f"Connecting to {address}:{port} ({connection_type})")
            node.connect_node_by_url(
                url=f"{address}:{port}", method=method, v2transport=supports_v2_p2p
            )

        if supports_v2_p2p is None:
            supports_v2_p2p = (
                node.use_v2transport if hasattr(node, "use_v2transport") else False
            )
        if advertise_v2_p2p is None:
            advertise_v2_p2p = (
                node.use_v2transport if hasattr(node, "use_v2transport") else False
            )

        # Handle v2 P2P advertisement
        if advertise_v2_p2p:
            kwargs["services"] = kwargs.get("services", P2P_SERVICES) | NODE_P2P_V2

        # If advertised v2 but doesn't support it, reconnection needed
        reconnect = advertise_v2_p2p and not supports_v2_p2p
        supports_v2_p2p = supports_v2_p2p and advertise_v2_p2p

        p2p_conn.peer_accept_connection(
            connect_cb=add_connection_callback,
            connect_id=p2p_idx + 1,
            net=node.chain if hasattr(node, "chain") else "regtest",
            timeout_factor=1.0,
            supports_v2_p2p=supports_v2_p2p,
            reconnect=reconnect,
            **kwargs,
        )()

        if reconnect:
            p2p_conn.wait_for_reconnect()

        if connection_type == "feeler" or wait_for_disconnect:
            p2p_conn.wait_until(
                lambda: p2p_conn.message_count.get("version", 0) == 1,
                check_connected=False,
            )
            p2p_conn.wait_until(
                lambda: not p2p_conn.is_connected, check_connected=False
            )
        else:
            p2p_conn.wait_for_connect()
            self._p2p_interface.append((node, p2p_conn))

            if supports_v2_p2p:
                p2p_conn.wait_until(lambda: p2p_conn.v2_state.tried_v2_handshake)
            p2p_conn.wait_until(lambda: not p2p_conn.on_connection_send_msg)
            if wait_for_verack:
                p2p_conn.wait_for_verack()
                p2p_conn.sync_with_ping()

        wait_until(predicate=lambda: node.rpc.get_connectioncount() == node_peers + 1)

        return p2p_conn

    def add_p2p_connection_default(
        self,
        node: Node,
        *,
        wait_for_verack=True,
        p2p_idx,
        connection_type="outbound-full-relay",
        supports_v2_p2p=True,
        advertise_v2_p2p=True,
        method="onetry",
        **kwargs,
    ):
        """Add an outbound p2p connection with a default P2PInterface.

        This method creates a default P2PInterface and connects it to the first node.

        Parameters:
            p2p_idx: Index for the connection
            connection_type: Type of connection
            supports_v2_p2p: whether to support v2 P2P
            advertise_v2_p2p: whether to advertise v2 P2P support

        Returns:
            The P2PInterface object
        """
        # Create default P2PInterface
        p2p_interface = P2PInterface()

        # Use the custom version to do the actual connection
        return self.add_p2p_connection(
            node=node,
            p2p_conn=p2p_interface,
            wait_for_verack=wait_for_verack,
            p2p_idx=p2p_idx,
            connection_type=connection_type,
            supports_v2_p2p=supports_v2_p2p,
            advertise_v2_p2p=advertise_v2_p2p,
            method=method,
            **kwargs,
        )

    def create_msg_random(self, msgtype, size: int):
        """
        Create a message of a given size.
        """
        oversized_payload = b"\x00" * size

        # Create a generic message
        return msg_generic(msgtype, oversized_payload)

    def create_node_address(self, quantity: int):
        """
        Create a list of node addresses.
        """

        i2p_addr = "c4gfnttsuwqomiygupdqqqyy5y5emnk5c73hrfvatri67prd7vyq.b32.i2p"
        onion_addr = "nix2iapg23s2g6tog6vmmr2xgywfly5522c27hnp7qwm5qyk73mufvyd.onion"

        address_list = []
        for i in range(quantity):
            addr = CAddress()
            addr.time = int(time.time()) + i
            addr.port = 8333 + i
            addr.nServices = P2P_SERVICES
            # Add one I2P and one onion V3 address at an arbitrary position.
            if i % 5 == 0:
                addr.net = addr.NET_I2P
                addr.ip = i2p_addr
                addr.port = 0
            elif i % 3 == 0:
                addr.net = addr.NET_TORV3
                addr.ip = onion_addr
            elif i % 2 == 0:
                addr.net = addr.NET_IPV6
                addr.ip = f"2001:db8::{i % 65536}"
            else:
                addr.ip = f"192.42.116.{i % 256}"
            address_list.append(addr)

        return address_list
