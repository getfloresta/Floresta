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
import random
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
from test_framework.util import Utility


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
        timeout = time.time() + 30
        while time.time() < timeout:
            if self.check_connection(peer_one, peer_two, is_connected):
                self.log.debug(
                    f"Peers {peer_one.variant} and {peer_two.variant} are in the expected "
                    f"connection state."
                )
                return

            if attempts < 10:
                time.sleep(1)
            else:
                time.sleep(2)

            attempts += 1

            # Send a ping to both peers to trigger a peer state update
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

        raise AssertionError(
            f"Peers {peer_one.variant} and {peer_two.variant} failed to reach the expected "
            f"connection state within the timeout. Expected connected: {is_connected}."
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
