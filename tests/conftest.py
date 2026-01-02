"""
Pytest configuration and fixtures for node testing.

This module provides fixtures for creating and managing test nodes
(florestad, bitcoind, utreexod) in various configurations.
"""

# pylint: disable=redefined-outer-name

import builtins
import os
import logging
import pytest

from test_framework import FlorestaTestFramework
from test_framework.node import Node, NodeType
from test_framework.util import Utility

# defaults to import...
GENESIS_BLOCK_HEIGHT = 0
GENESIS_BLOCK_BLOCK = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
GENESIS_BLOCK_DIFFICULTY_INT = 1
GENESIS_BLOCK_DIFFICULTY_FLOAT = 4.656542373906925e-10
GENESIS_BLOCK_LEAF_COUNT = 0
TEST_CHAIN = "regtest"
FLORESTA_TEMP_DIR = os.getenv("FLORESTA_TEMP_DIR")


@pytest.fixture(scope="session", autouse=True)
def validate_and_check_environment():
    """Validate environment and check for required binaries before running tests."""
    temp_dir = FLORESTA_TEMP_DIR
    if not temp_dir:
        pytest.fail("FLORESTA_TEMP_DIR environment variable not set")

    if not os.path.exists(temp_dir):
        pytest.fail(f"FLORESTA_TEMP_DIR directory does not exist: {temp_dir}")

    # Create necessary subdirectories
    os.makedirs(os.path.join(temp_dir, "logs"), exist_ok=True)
    os.makedirs(os.path.join(temp_dir, "data"), exist_ok=True)

    # Check for required binaries
    binaries_dir = os.path.join(temp_dir, "binaries")
    binaries = {
        "florestad": os.path.join(binaries_dir, "florestad"),
        "utreexod": os.path.join(binaries_dir, "utreexod"),
        "bitcoind": os.path.join(binaries_dir, "bitcoind"),
    }

    for binary_name, binary_path in binaries.items():
        if not os.path.exists(binary_path):
            pytest.fail(f"{binary_name} binary not found at {binary_path}")


# pylint: disable=W0511
# TODO: remove this function after all tests are migrated to the pytest standard
# so that instead of other functions using print they should use logging directly
def redirect_print_to_logger(logger):
    """
    Replace the built-in print function to redirect messages to the logger.
    """
    original_print = builtins.print

    # pylint: disable=unused-argument
    def custom_print(*args, **kwargs):
        # Convert print arguments into a single string
        message = " ".join(map(str, args))
        # Redirect to the logger at DEBUG level
        logger.debug(message)

    # Replace the print function with custom_print
    builtins.print = custom_print

    return original_print  # Return the original print function in case it needs to be restored


@pytest.fixture(scope="function")
def setup_logging(request):
    """
    Configure logging for the test, including the file and line number where the log was called.
    """
    logger = logging.getLogger(request.node.name)

    # Log format to include the file and line
    formatter = logging.Formatter(
        "%(asctime)s - %(levelname)s - %(filename)s:%(lineno)d - %(message)s"
    )

    # Configure console handler
    console_handler = logging.StreamHandler()
    console_handler.setFormatter(formatter)

    # Configure log file
    git_describe = Utility.get_git_describe()
    log_file = os.path.join(
        FLORESTA_TEMP_DIR, "logs", git_describe, f"{request.node.name}.log"
    )
    os.makedirs(os.path.dirname(log_file), exist_ok=True)
    file_handler = logging.FileHandler(log_file, mode="w")
    file_handler.setFormatter(formatter)

    # Add handlers to the logger
    if not logger.handlers:
        logger.addHandler(console_handler)
        logger.addHandler(file_handler)

    # Redirect print to the logger
    original_print = redirect_print_to_logger(logger)

    yield logger

    # Restore the original print after the test
    builtins.print = original_print

    # Clear handlers after the test
    logger.handlers.clear()


@pytest.fixture(scope="function")
def node_manager(setup_logging, request):
    """Provides a FlorestaTestFramework instance that automatically cleans up after each test"""
    manager = FlorestaTestFramework(logger=setup_logging, test_name=request.node.name)

    yield manager

    # Cleanup happens automatically after yield
    manager.stop()


@pytest.fixture
def florestad_node(node_manager) -> Node:
    """Single `florestad` node with default configurations, started and ready for testing"""
    node = node_manager.add_node_default_args(variant=NodeType.FLORESTAD)
    node_manager.run_node(node)
    return node


@pytest.fixture
def bitcoind_node(node_manager) -> Node:
    """Single `bitcoind` node with default configurations, started and ready for testing"""
    node = node_manager.add_node_default_args(variant=NodeType.BITCOIND)
    node_manager.run_node(node)
    return node


@pytest.fixture
def utreexod_node(node_manager) -> Node:
    """Single `utreexod` node with default configurations, started and ready for testing"""
    node = node_manager.add_node_extra_args(
        variant=NodeType.UTREEXOD,
        extra_args=[
            "--miningaddr=bcrt1q4gfcga7jfjmm02zpvrh4ttc5k7lmnq2re52z2y",
            "--utreexoproofindex",
            "--prune=0",
        ],
    )
    node_manager.run_node(node)
    return node


@pytest.fixture
def florestad_utreexod(
    florestad_node, utreexod_node, node_manager
) -> tuple[Node, Node]:
    """
    Creates and starts a `florestad` node and a `utreexod` node.
    The nodes are automatically connected to each other and are ready for testing.
    """
    florestad = florestad_node
    utreexod = utreexod_node

    node_manager.connect_nodes(florestad, utreexod)

    return florestad, utreexod


@pytest.fixture
def add_node_with_tls(node_manager):
    """Creates and starts a node with TLS enabled, based on the specified variant."""

    def _create_node(variant: NodeType) -> Node:
        if variant == NodeType.BITCOIND:
            raise ValueError("BITCOIND does not support TLS")

        node = node_manager.add_node_with_tls(
            variant=variant,
        )
        node_manager.run_node(node)
        return node

    return _create_node


@pytest.fixture
def add_node_with_extra_args(node_manager):
    """
    Creates and starts a node with extra command-line arguments, based on the
    specified variant.
    """

    def _create_node(variant: NodeType, extra_args: list) -> Node:
        node = node_manager.add_node_extra_args(
            variant=variant,
            extra_args=extra_args,
        )
        node_manager.run_node(node)
        return node

    return _create_node
