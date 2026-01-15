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
from datetime import datetime, timezone
from typing import Any, Dict, List, Pattern

from test_framework.crypto.pkcs8 import (
    create_pkcs8_private_key,
    create_pkcs8_self_signed_certificate,
)
from test_framework.daemon.bitcoin import BitcoinDaemon
from test_framework.daemon.floresta import FlorestaDaemon
from test_framework.daemon.utreexo import UtreexoDaemon
from test_framework.rpc.bitcoin import BitcoinRPC
from test_framework.rpc.floresta import FlorestaRPC
from test_framework.rpc.utreexo import UtreexoRPC
from test_framework.rpc.bitcoin import REGTEST_RPC_SERVER as bitcoind_rpc_server
from test_framework.rpc.floresta import REGTEST_RPC_SERVER as florestad_rpc_server
from test_framework.rpc.utreexo import REGTEST_RPC_SERVER as utreexod_rpc_server


class Node:
    """
    A node object to be used in the test framework.
    It contains the `daemon`, `rpc` and `rpc_config` objects.
    """

    def __init__(self, daemon, rpc, rpc_config, variant):
        self.daemon = daemon
        self.rpc = rpc
        self.rpc_config = rpc_config
        self.variant = variant

    def start(self):
        """
        Start the node.
        """
        if self.daemon.is_running:
            raise RuntimeError(f"Node '{self.variant}' is already running.")
        self.daemon.start()
        self.rpc.wait_for_connections(opened=True)

    def stop(self):
        """
        Stop the node.
        """
        response = None
        if self.daemon.is_running:
            try:
                response = self.rpc.stop()
            # pylint: disable=broad-exception-caught
            except Exception:
                self.daemon.process.terminate()

            self.daemon.process.wait()
            self.rpc.wait_for_connections(opened=False)

        return response

    def connect(self, other_node: "Node"):
        """
        Connect this node to another node using `addnode` RPC command.
        """
        if self.daemon.is_running is False:
            raise RuntimeError(f"Node '{self.variant}' is not running.")

        if other_node.daemon.is_running is False:
            raise RuntimeError(f"Node '{other_node.variant}' is not running.")

        address = f"127.0.0.1:{other_node.get_port('p2p')}"
        response = self.rpc.addnode(address, "add")

        if response is not None:
            raise RuntimeError(
                f"Failed to connect {self.variant} to {other_node.variant}, "
                f"port {other_node.get_ports('p2p')}"
            )

    def get_host(self) -> str:
        """
        Get the host address of the node.
        """
        return self.rpc_config["host"]

    def get_ports(self) -> int:
        """Get all ports of the node."""
        return self.rpc_config["ports"]

    def get_port(self, port_type: str) -> int:
        """
        Get the port of the node based on the port type.
        This is a convenience method for `get_ports`.
        """
        if port_type not in self.rpc_config["ports"]:
            raise ValueError(
                f"Port type '{port_type}' not found in node ports: {self.rpc_config['ports']}"
            )
        return self.rpc_config["ports"][port_type]

    def send_kill_signal(self, sigcode="SIGTERM"):
        """Send a signal to kill the daemon process."""
        with contextlib.suppress(ProcessLookupError):
            pid = self.daemon.process.pid
            os.kill(pid, getattr(signal, sigcode, signal.SIGTERM))


# pylint: disable=too-many-public-methods
class FlorestaTestFramework:
    """
    A utility framework designed to manage and simplify the setup, execution,
    and teardown of nodes in Floresta's testing environment.

     This framework provides tools to:
    - Start and stop nodes (`florestad`, `utreexod`, and `bitcoind`) with custom configurations.
    - Manage node connections, ensuring proper communication between nodes.
    - Create TLS certificates for secure communication when required.
    - Facilitate integration and functional tests by providing reusable methods
      to interact with nodes and their RPC interfaces.

    This class is intended to be used as a base for Floresta's test scripts,
    providing a consistent and extensible structure for managing nodes and
    running tests.
    """

    def __init__(self, logger, test_name):
        """
        Sets test framework defaults.
        """
        self._test_name = test_name
        self._nodes = []
        self.log = logger

    @property
    def test_name(self) -> str:
        """
        Get the test name, which is the class name in lowercase.
        This is used to create a log file for the test.
        """
        if self._test_name is not None:
            return self._test_name

        self._test_name = self.__class__.__name__.lower()
        return self._test_name

    @staticmethod
    def get_integration_test_dir():
        """
        Get path for florestad used in integration tests, generally set on
        $FLORESTA_TEMP_DIR/binaries
        """
        if os.getenv("FLORESTA_TEMP_DIR") is None:
            raise RuntimeError(
                "FLORESTA_TEMP_DIR not set. "
                + " Please set it to the path of the integration test directory."
            )
        return os.getenv("FLORESTA_TEMP_DIR")

    @staticmethod
    def create_data_dirs(data_dir: str, base_name: str, nodes: int) -> list[str]:
        """
        Create the data directories for any nodes to be used in the test.
        """
        paths = []
        for i in range(nodes):
            p = os.path.join(data_dir, "data", base_name, f"node-{i}")
            os.makedirs(p, exist_ok=True)
            paths.append(p)

        return paths

    @staticmethod
    def get_available_random_port(start: int, end: int = 65535):
        """Get an available random port in the range [start, end]"""
        while True:
            port = random.randint(start, end)
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                # Check if the port is available
                if s.connect_ex(("127.0.0.1", port)) != 0:
                    return port

    @staticmethod
    def get_random_port():
        """Get a random port in the range [2000, 65535]"""
        return FlorestaTestFramework.get_available_random_port(2000, 65535)

    def create_tls_key_cert(self) -> tuple[str, str]:
        """
        Create a PKCS#8 formatted private key and a self-signed certificate.
        These keys are intended to be used with florestad's --tls-key-path and --tls-cert-path
        options.
        """
        # If we're in CI, we need to use the
        # path to the integration test dir
        # tempfile will be used to get the proper
        # temp dir for the OS
        tls_rel_path = os.path.join(
            FlorestaTestFramework.get_integration_test_dir(), "data", "tls"
        )
        tls_path = os.path.normpath(os.path.abspath(tls_rel_path))

        # Create the folder if not exists
        os.makedirs(tls_path, exist_ok=True)

        # Create certificates
        pk_path, private_key = create_pkcs8_private_key(tls_path)
        self.log.debug(f"Created PKCS#8 key at {pk_path}")

        cert_path = create_pkcs8_self_signed_certificate(
            tls_path, private_key, common_name="florestad", validity_days=365
        )
        self.log.debug(f"Created self-signed certificate at {cert_path}")

        return (pk_path, cert_path)

    def is_option_set(self, extra_args: list[str], option: str) -> bool:
        """
        Check if an option is set in extra_args
        """

        return any(arg.startswith(option) for arg in extra_args)

    def extract_port_from_args(self, extra_args: list[str], option: str) -> int:
        """Extract port number from command-line arguments."""
        return any(arg.startswith(option) for arg in extra_args)

    def should_enable_electrum_for_utreexod(self, extra_args: list[str]) -> bool:
        """Determine if electrum should be enabled for utreexod."""
        electrum_disabled_options = [
            "--noelectrum",
            "--disable-electrum",
            "--electrum=false",
            "--electrum=0",
        ]
        if any(
            arg.startswith(opt)
            for arg in extra_args
            for opt in electrum_disabled_options
        ):
            return False

        electrum_listener_options = ["--electrumlisteners", "--tlselectrumlisteners"]
        return any(
            arg.startswith(opt)
            for arg in extra_args
            for opt in electrum_listener_options
        )

    # pylint: disable=too-many-arguments,too-many-positional-arguments
    def create_data_dir_for_daemon(
        self,
        data_dir_arg: str,
        default_args: list[str],
        extra_args: list[str],
        tempdir: str,
        testname: str,
    ):
        """
        Create a data directory for the daemon to be run.
        """
        # Add a default data-dir if not set
        if not self.is_option_set(extra_args, data_dir_arg):
            datadir = os.path.normpath(os.path.join(tempdir, "data", testname))
            default_args.append(f"{data_dir_arg}={datadir}")

        else:
            data_dir_arg = next(
                (arg for arg in extra_args if arg.startswith(f"{data_dir_arg}="))
            )
            datadir = data_dir_arg.split("=", 1)[1]

        if not os.path.exists(datadir):
            self.log.debug(f"Creating data directory for {data_dir_arg} in {datadir}")
            os.makedirs(datadir, exist_ok=True)

    # pylint: disable=too-many-positional-arguments,too-many-arguments
    def setup_florestad_daemon(
        self,
        targetdir: str,
        tempdir: str,
        testname: str,
        extra_args: List[str],
        tls: bool,
    ) -> Node:
        """Add default args to a florestad node settings to be run and return a Node object."""
        daemon = FlorestaDaemon(self.log)
        daemon.create(target=targetdir)
        default_args = []
        ports = {}

        self.create_data_dir_for_daemon(
            "--data-dir", default_args, extra_args, tempdir, testname
        )

        if not self.is_option_set(extra_args, "--rpc-address"):
            ports["rpc"] = self.get_random_port()
            default_args.append(f"--rpc-address=127.0.0.1:{ports['rpc']}")
        else:
            ports["rpc"] = self.extract_port_from_args(extra_args, "--rpc-address")

        if not self.is_option_set(extra_args, "--electrum-address"):
            ports["electrum-server"] = self.get_random_port()
            default_args.append(
                f"--electrum-address=127.0.0.1:{ports['electrum-server']}"
            )
        else:
            ports["electrum-server"] = self.extract_port_from_args(
                extra_args, "--electrum-address"
            )

        if tls:
            key, cert = self.create_tls_key_cert()
            default_args.append("--enable-electrum-tls")
            default_args.append(f"--tls-key-path={key}")
            default_args.append(f"--tls-cert-path={cert}")

            if not self.is_option_set(extra_args, "--electrum-address-tls"):
                ports["electrum-server-tls"] = self.get_random_port()
                default_args.append(
                    f"--electrum-address-tls=127.0.0.1:{ports['electrum-server-tls']}"
                )
            else:
                ports["electrum-server-tls"] = self.extract_port_from_args(
                    extra_args, "--electrum-address-tls"
                )

        daemon.add_daemon_settings(default_args + extra_args)
        rpcserver = copy.deepcopy(florestad_rpc_server)
        rpcserver["ports"] = ports
        return Node(daemon, None, rpcserver, "florestad")

    # pylint: disable=too-many-arguments,too-many-positional-arguments
    def setup_utreexod_daemon(
        self,
        targetdir: str,
        tempdir: str,
        testname: str,
        extra_args: List[str],
        tls: bool,
    ) -> Node:
        """Add default args to a utreexod node settings to be run and return a Node object."""
        daemon = UtreexoDaemon(self.log)
        daemon.create(target=targetdir)
        default_args = []
        ports = {}

        self.create_data_dir_for_daemon(
            "--datadir", default_args, extra_args, tempdir, testname
        )
        if not self.is_option_set(extra_args, "--listen"):
            ports["p2p"] = self.get_random_port()
            default_args.append(f"--listen=127.0.0.1:{ports['p2p']}")
        else:
            ports["p2p"] = self.extract_port_from_args(extra_args, "--listen")

        if not self.is_option_set(extra_args, "--rpclisten"):
            ports["rpc"] = self.get_random_port()
            default_args.append(f"--rpclisten=127.0.0.1:{ports['rpc']}")
        else:
            ports["rpc"] = self.extract_port_from_args(extra_args, "--rpclisten")

        electrum_enabled = self.should_enable_electrum_for_utreexod(extra_args)

        if electrum_enabled and self.is_option_set(extra_args, "--electrumlisteners"):
            ports["electrum-server"] = self.extract_port_from_args(
                extra_args, "--electrumlisteners"
            )

        if tls:
            key, cert = self.create_tls_key_cert()
            default_args.extend([f"--rpckey={key}", f"--rpccert={cert}"])

            if electrum_enabled and self.is_option_set(
                extra_args, "--tlselectrumlisteners"
            ):
                ports["electrum-server-tls"] = self.extract_port_from_args(
                    extra_args, "--tlselectrumlisteners"
                )
        else:
            default_args.append("--notls")

        daemon.add_daemon_settings(default_args + extra_args)
        rpcserver = copy.deepcopy(utreexod_rpc_server)
        rpcserver["ports"] = ports
        return Node(daemon, None, rpcserver, "utreexod")

    # pylint: disable=too-many-arguments,too-many-positional-arguments
    def setup_bitcoind_daemon(
        self,
        targetdir: str,
        tempdir: str,
        testname: str,
        extra_args: List[str],
    ) -> Node:
        """Add default args to a bitcoind node settings to be run and return a Node object."""
        daemon = BitcoinDaemon(self.log)
        daemon.create(target=targetdir)
        default_args = []
        ports = {}

        self.create_data_dir_for_daemon(
            "-datadir", default_args, extra_args, tempdir, testname
        )

        if not self.is_option_set(extra_args, "-bind"):
            ports["p2p"] = self.get_random_port()
            default_args.append(f"-bind=127.0.0.1:{ports['p2p']}")
        else:
            ports["p2p"] = self.extract_port_from_args(extra_args, "-bind")

        if not self.is_option_set(extra_args, "-rpcbind"):
            ports["rpc"] = self.get_random_port()
            default_args.extend(
                ["-rpcallowip=127.0.0.1", f"-rpcbind=127.0.0.1:{ports['rpc']}"]
            )
        else:
            ports["rpc"] = self.extract_port_from_args(extra_args, "-rpcbind")

        daemon.add_daemon_settings(default_args + extra_args)
        rpcserver = copy.deepcopy(bitcoind_rpc_server)
        rpcserver["ports"] = ports
        return Node(daemon, None, rpcserver, "bitcoind")

    # pylint: disable=dangerous-default-value
    def add_node(
        self,
        extra_args: List[str] = [],
        variant: str = "florestad",
        tls: bool = False,
    ) -> Node:
        """
        Add a node settings to be run. Use this on set_test_params method
        many times you want. Extra_args should be a list of string in the
        --key=value strings (see florestad --help for a list of available
        commands)
        """
        tempdir = str(self.get_integration_test_dir())
        targetdir = os.path.join(tempdir, "binaries")
        testname = self.test_name

        if variant == "florestad":
            node = self.setup_florestad_daemon(
                targetdir, tempdir, testname, extra_args, tls
            )
        elif variant == "utreexod":
            node = self.setup_utreexod_daemon(
                targetdir, tempdir, testname, extra_args, tls
            )
        elif variant == "bitcoind":
            node = self.setup_bitcoind_daemon(targetdir, tempdir, testname, extra_args)
        else:
            raise ValueError(f"Unsupported variant: {variant}")

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

    def run_node(self, node: Node, timeout: int = 180):
        """Start a node and initialize its RPC connection."""
        node.daemon.start()

        if node.variant == "florestad":
            node.rpc = FlorestaRPC(self.log, node.daemon.process, node.rpc_config)
        elif node.variant == "utreexod":
            node.rpc = UtreexoRPC(self.log, node.daemon.process, node.rpc_config)
        elif node.variant == "bitcoind":
            node.rpc = BitcoinRPC(self.log, node.daemon.process, node.rpc_config)

        node.rpc.wait_for_connections(opened=True, timeout=timeout)
        self.log.debug(
            f"Node '{node.variant}' started on ports: {node.rpc_config['ports']}"
        )

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

    def connect_nodes(self):
        """
        Establish connections between nodes in the test framework.
        - Connects `florestad` to non-Floresta nodes.
        - Ensures no redundant connections between non-Floresta nodes.
        - Avoids self-connections and treats (i, j) and (j, i) as the same pair.
        """
        if len(self._nodes) < 2:
            raise RuntimeError("Not enough nodes to connect.")

        florestads = [node for node in self._nodes if node.variant == "florestad"]
        other_nodes = [node for node in self._nodes if node.variant != "florestad"]

        if len(other_nodes) < 1:
            raise RuntimeError("No non-Floresta nodes available to connect.")

        for florestad_index, florestad_node in enumerate(florestads):
            for other_node_index, other_node in enumerate(other_nodes):
                florestad_node.connect(other_node)
                self.log.debug(
                    f"Connected Floresta node {florestad_index} to node {other_node_index}"
                )

        if len(other_nodes) < 2:
            return

        connected_pairs = set()

        for source_node_index, source_node in enumerate(other_nodes):
            for target_node_index, target_node in enumerate(other_nodes):
                if source_node_index == target_node_index:
                    continue
                pair = (
                    min(source_node_index, target_node_index),
                    max(source_node_index, target_node_index),
                )
                if pair in connected_pairs:
                    continue
                source_node.connect(target_node)
                connected_pairs.add(pair)
                self.log.debug(
                    f"Connected non-Floresta node {source_node_index} to node {target_node_index}"
                )
