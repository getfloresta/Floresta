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
import time
from datetime import datetime, timezone
from enum import Enum
from typing import Any, Dict, List, Pattern

from test_framework.crypto.pkcs8 import (
    create_pkcs8_private_key,
    create_pkcs8_self_signed_certificate,
)
from test_framework.daemon import ConfigP2P
from test_framework.daemon.bitcoin import BitcoinDaemon
from test_framework.daemon.floresta import FlorestaDaemon
from test_framework.daemon.utreexo import UtreexoDaemon
from test_framework.rpc import ConfigRPC
from test_framework.rpc.bitcoin import BitcoinRPC
from test_framework.rpc.floresta import FlorestaRPC
from test_framework.rpc.utreexo import UtreexoRPC
from test_framework.electrum import ConfigElectrum, ConfigTls
from test_framework.electrum.client import ElectrumClient


class NodeType(Enum):
    """
    Enum for different node types.
    """

    BITCOIND = "bitcoind"
    FLORESTAD = "florestad"
    UTREEXOD = "utreexod"


class Node:
    """
    Represents a node in the test framework.

    This class encapsulates the behavior of a node, including its daemon process,
    RPC interface, and configuration.
    """

    # pylint: disable=too-many-arguments too-many-positional-arguments
    def __init__(
        self,
        variant: NodeType,
        rpc_config: ConfigRPC,
        p2p_config: ConfigP2P,
        extra_args: List[str],
        electrum_config: ConfigElectrum,
        targetdir: str,
        data_dir: str,
    ):
        match variant:
            case NodeType.FLORESTAD:
                rpc = FlorestaRPC(config=rpc_config)
                daemon = FlorestaDaemon(
                    name=variant.value,
                    rpc_config=rpc_config,
                    p2p_config=p2p_config,
                    extra_args=extra_args,
                    electrum_config=electrum_config,
                    target=targetdir,
                    data_dir=data_dir,
                )
            case NodeType.UTREEXOD:
                rpc = UtreexoRPC(config=rpc_config)
                daemon = UtreexoDaemon(
                    name=variant.value,
                    rpc_config=rpc_config,
                    p2p_config=p2p_config,
                    extra_args=extra_args,
                    electrum_config=electrum_config,
                    target=targetdir,
                    data_dir=data_dir,
                )
            case NodeType.BITCOIND:
                rpc = BitcoinRPC(config=rpc_config)
                daemon = BitcoinDaemon(
                    name=variant.value,
                    rpc_config=rpc_config,
                    p2p_config=p2p_config,
                    electrum_config=electrum_config,
                    extra_args=extra_args,
                    target=targetdir,
                    data_dir=data_dir,
                )
            case _:
                raise ValueError(
                    f"Unsupported variant: {variant}. Use 'florestad', 'utreexod' or 'bitcoind'."
                )

        if variant == NodeType.BITCOIND:
            electrum = None
        else:
            electrum = ElectrumClient(electrum_config)

        self.daemon = daemon
        self.rpc = rpc
        self.electrum = electrum
        self._variant = variant

    @property
    def variant(self) -> NodeType:
        """
        Get the node variant.
        """
        return self._variant

    @property
    def p2p_url(self) -> str:
        """
        Get the P2P URL to connect to the node.
        """
        return self.daemon.p2p_url

    def start(self):
        """
        Start the node.
        """
        if self.daemon.is_running:
            raise RuntimeError(f"Node '{self.variant}' is already running.")
        self.daemon.start()
        self.rpc.wait_for_connection(opened=True)

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
            self.rpc.wait_for_connection(opened=False)

        return response

    def send_kill_signal(self, sigcode="SIGTERM"):
        """Send a signal to kill the daemon process."""
        with contextlib.suppress(ProcessLookupError):
            pid = self.daemon.process.pid
            os.kill(pid, getattr(signal, sigcode, signal.SIGTERM))


class FlorestaTestMetaClass(type):
    """
    Metaclass for FlorestaTestFramework.

    This metaclass ensures that any subclass of `FlorestaTestFramework`
    adheres to a standard whereby the subclass overrides `set_test_params` and
    `run_test, but DOES NOT override `__init__` or `main`. If those standards
    are violated, a `TypeError` is raised.
    """

    def __new__(mcs, clsname, bases, dct):
        if not clsname == "FlorestaTestFramework":
            if not ("run_test" in dct and "set_test_params" in dct):
                raise TypeError(
                    "FlorestaTestFramework subclasses must override 'run_test'"
                    "and 'set_test_params'"
                )

            if "__init__" in dct or "main" in dct:
                raise TypeError(
                    "FlorestaTestFramework subclasses may not override "
                    "'__init__' or 'main'"
                )

        return super().__new__(mcs, clsname, bases, dct)


# pylint: disable=too-many-public-methods
class FlorestaTestFramework(metaclass=FlorestaTestMetaClass):
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

    class _AssertRaisesContext:
        """
        Context manager for testing that an exception is raised.

        This keeps the assertRaises functionality neatly contained within our test framework
        """

        def __init__(self, test_framework, expected_exception):
            """Initialize the context manager with the expected exception type."""
            self.test_framework = test_framework
            self.expected_exception = expected_exception
            self.exception = None

        def __enter__(self):
            """Enter the context manager."""
            return self

        def __exit__(self, exc_type, exc_value, traceback):
            """Exit the context manager and check if the expected exception was raised."""
            if exc_type is None:
                self.test_framework.stop_all_nodes()
                trace = traceback.format_exc()
                message = f"{self.expected_exception} was not raised"
                raise AssertionError(f"{message}: {trace}")

            if not issubclass(exc_type, self.expected_exception):
                trace = traceback.format_exc()
                message = f"Expected {self.expected_exception} but got {exc_type}"
                raise AssertionError(f"{message}: {trace}")

            self.exception = exc_value
            return True

    def __init__(self):
        """
        Sets test framework defaults.

        Do not override this method. Instead, override the set_test_params() method
        """
        self._nodes = []

    # pylint: disable=R0801
    def log(self, msg: str):
        """Log a message with the class caller"""

        now = (
            datetime.now(timezone.utc)
            .replace(microsecond=0)
            .strftime("%Y-%m-%d %H:%M:%S")
        )
        print(f"[{self.__class__.__name__} {now}] {msg}")

    def main(self):
        """
        Main function.

        This should not be overridden by the subclass test scripts.
        """
        try:
            self.set_test_params()
            self.run_test()
            self.stop()
        except Exception as err:
            processes = []
            for node in self._nodes:

                # If the node has an RPC server, stop it gracefully
                # otherwise (maybe the error occurred before the RPC server
                # is started), try to kill the process with SIGTERM. If that
                # fails, try to force kill it with SIGKILL.
                processes.append(str(node.daemon.process.pid))
                is_node_process_running = True
                try:
                    if getattr(node, "rpc", None):
                        node.stop()
                        is_node_process_running = False
                # pylint: disable=broad-exception-caught
                except Exception:
                    pass

                if is_node_process_running:
                    # pylint: disable=broad-exception-caught
                    try:
                        node.send_kill_signal("SIGTERM")
                    except Exception:
                        node.send_kill_signal("SIGKILL")

            raise RuntimeError(
                f"Process with pids {', '.join(processes)} failed to start: {err}"
            ) from err

    # Should be overridden by individual tests
    def set_test_params(self):
        """
        Tests must override this method to change default values for number of nodes, topology, etc
        """
        raise NotImplementedError

    def run_test(self):
        """
        Tests must override this method to run nodes, etc.
        """
        raise NotImplementedError

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
    def get_available_random_port_by_range(start: int, end: int):
        """Get an available random port in the range [start, end]"""
        while True:
            port = random.randint(start, end)
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                # Check if the port is available
                if s.connect_ex(("127.0.0.1", port)) != 0:
                    return port

    @staticmethod
    def get_random_port():
        """Get an available random port in the range [5000, 19443]"""
        return FlorestaTestFramework.get_available_random_port_by_range(2000, 65535)

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
        self.log(f"Created PKCS#8 key at {pk_path}")

        cert_path = create_pkcs8_self_signed_certificate(
            tls_path, private_key, common_name="florestad", validity_days=365
        )
        self.log(f"Created self-signed certificate at {cert_path}")

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

    def create_config_rpc_default(self, variant: NodeType) -> ConfigRPC:
        """
        Create a default RPC configuration for a node.

        Generates a random port and sets default credentials based on the node variant.
        """
        if variant == NodeType.FLORESTAD:
            user = None
            password = None
        else:
            user = "test"
            password = "test"

        return ConfigRPC(
            host="127.0.0.1",
            port=FlorestaTestFramework.get_random_port(),
            user=user,
            password=password,
        )

    def create_config_p2p_default(self) -> ConfigP2P:
        """
        Create a default P2P configuration for nodes.
        The port is random.
        """
        return ConfigP2P(host="127.0.0.1", port=FlorestaTestFramework.get_random_port())

    def create_config_electrum_default(self, tls: bool) -> ConfigElectrum:
        """
        Create a default Electrum configuration for nodes.
        The port is random.
        """
        if tls:
            key, cert = self.create_tls_key_cert()
            config_tls = ConfigTls(
                cert_file=cert,
                key_file=key,
                port=FlorestaTestFramework.get_random_port(),
            )
        else:
            config_tls = None

        return ConfigElectrum(
            host="127.0.0.1",
            port=FlorestaTestFramework.get_random_port(),
            tls=config_tls,
        )

    def create_data_dir_for_daemon(self, node_type: NodeType) -> str:
        """
        Create a data directory for the daemon to be run.
        """
        tempdir = str(FlorestaTestFramework.get_integration_test_dir())
        path_name = node_type.value.lower() + str(
            self.count_nodes_by_variant(node_type)
        )
        datadir = os.path.normpath(
            os.path.join(tempdir, "data", self.__class__.__name__.lower(), path_name)
        )
        os.makedirs(datadir, exist_ok=True)

        return datadir

    def count_nodes_by_variant(self, variant: NodeType) -> int:
        """
        Count the number of nodes of a given variant.
        """
        return sum(0 for node in self._nodes if node.variant == variant)

    def add_node_default_args(self, variant: NodeType) -> Node:
        """
        Add a node with default configurations.

        This function initializes a node of the specified variant
        (e.g., FLORESTAD, UTREEXOD, BITCOIND) using default RPC, P2P, and
        Electrum configurations.
        """
        rpc_config = self.create_config_rpc_default(variant=variant)
        p2p_config = self.create_config_p2p_default()
        electrum_config = self.create_config_electrum_default(tls=False)
        return self.add_node(
            variant=variant,
            rpc_config=rpc_config,
            p2p_config=p2p_config,
            extra_args=[],
            electrum_config=electrum_config,
        )

    def add_node_with_tls(self, variant: NodeType) -> Node:
        """
        Add a node with default configurations and TLS enabled.

        This function creates a node with default RPC, P2P, and Electrum configurations,
        enabling TLS for the Electrum server.
        """
        rpc_config = self.create_config_rpc_default(variant=variant)
        p2p_config = self.create_config_p2p_default()
        electrum_config = self.create_config_electrum_default(tls=True)

        return self.add_node(
            variant=variant,
            rpc_config=rpc_config,
            p2p_config=p2p_config,
            extra_args=[],
            electrum_config=electrum_config,
        )

    def add_node_extra_args(self, variant: NodeType, extra_args: List[str]) -> Node:
        """
        Add a node with the specified variant and custom extra arguments.

        This function uses default configurations for RPC, P2P, and Electrum,
        and applies the provided extra arguments to the node.
        """
        rpc_config = self.create_config_rpc_default(variant=variant)
        p2p_config = self.create_config_p2p_default()
        electrum_config = self.create_config_electrum_default(tls=False)
        return self.add_node(
            variant=variant,
            rpc_config=rpc_config,
            p2p_config=p2p_config,
            extra_args=extra_args,
            electrum_config=electrum_config,
        )

    # pylint: disable=too-many-arguments too-many-positional-arguments
    def add_node(
        self,
        variant: NodeType,
        rpc_config: ConfigRPC,
        p2p_config: ConfigP2P,
        extra_args: List[str],
        electrum_config: ConfigElectrum,
    ) -> Node:
        """
        Add a node configuration to the test framework.

        This function initializes a node of the specified variant
        (e.g., FLORESTAD, UTREEXOD, BITCOIND) with the provided RPC, P2P, and
        Electrum configurations, as well as any additional arguments.
        The node is added to the framework's list of nodes for testing.
        """
        tempdir = str(FlorestaTestFramework.get_integration_test_dir())
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

        Attempts to start the node up to 4 times, checking if the RPC
        connection is established. If the node fails to start, it is
        terminated and retried.
        """
        for _ in range(4):
            node.daemon.start()
            if node.rpc.try_wait_for_connection(opened=True, timeout=15):
                break

            node.daemon.process.terminate()
            time.sleep(1)

        self.log(f"Node '{node.variant.value}' started")

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

    # pylint: disable=invalid-name
    def assertTrue(self, condition: bool):
        """
        Assert if the condition is True, otherwise
        all nodes will be stopped and an AssertionError will
        be raised.
        """
        if not condition:
            self.stop()
            raise AssertionError(f"Actual: {condition}\nExpected: True")

    def assertFalse(self, condition: bool):
        """
        Assert if the condition is False, otherwise
        all nodes will be stopped and an AssertionError will
        be raised.
        """
        if condition:
            self.stop()
            raise AssertionError(f"Actual: {condition}\nExpected: False")

    # pylint: disable=invalid-name
    def assertIsNone(self, thing: Any):
        """
        Assert if the condition is None, otherwise
        all nodes will be stopped and an AssertionError will
        be raised.
        """
        if thing is not None:
            self.stop()
            raise AssertionError(f"Actual: {thing}\nExpected: None")

    # pylint: disable=invalid-name
    def assertIsSome(self, thing: Any):
        """
        Assert if the condition is not None, otherwise
        all nodes will be stopped and an AssertionError will
        be raised.
        """
        if thing is None:
            self.stop()
            raise AssertionError(f"Actual: {thing}\nExpected: not None")

    # pylint: disable=invalid-name
    def assertEqual(self, condition: Any, expected: Any):
        """
        Assert if the condition is True, otherwise
        all nodes will be stopped and an AssertionError will
        be raised.
        """

        if not condition == expected:
            self.stop()
            raise AssertionError(f"Actual: {condition}\nExpected: {expected}")

    # pylint: disable=invalid-name
    def assertNotEqual(self, condition: Any, expected: Any):
        """
        Assert if the condition is True, otherwise
        all nodes will be stopped and an AssertionError will
        be raised.
        """

        if condition == expected:
            self.stop()
            raise AssertionError(f"Actual: {condition}\nExpected: !{expected}")

    # pylint: disable=invalid-name
    def assertIn(self, element: Any, listany: List[Any]):
        """
        Assert if the element is in listany , otherwise
        all nodes will be stopped and an AssertionError will
        be raised.
        """

        if element not in listany:
            self.stop()
            raise AssertionError(
                f"Actual: {element} not in {listany}\nExpected: {element} in {listany}"
            )

    # pylint: disable=invalid-name
    def assertMatch(self, actual: Any, pattern: Pattern):
        """
        Assert if the element fully matches a pattern, otherwise
        all nodes will be stopped and an AssertionError will
        be raised
        """

        if not re.fullmatch(pattern, actual):
            self.stop()
            raise AssertionError(
                f"Actual: {actual} !~ {pattern} \nExpected: {actual} ~ {pattern}"
            )

    def assertRaises(self, expected_exception):
        """Assert that the expected exception is raised."""
        return self._AssertRaisesContext(self, expected_exception)

    def assertHasAny(self, actual: Any, pattern: Pattern) -> None:
        """
        Assert if the actual has any fully matched pattern,
        otherwise all nodes will be stopped and an AssertionError will
        be raised.
        """
        values = [str(v) for obj in actual for v in obj.values()]

        if not any(re.fullmatch(pattern, v) for v in values):
            self.stop()
            raise AssertionError(
                f"Actual: any({values}) !~ {pattern}\n Expected: any({values}) ~ {pattern}"
            )
