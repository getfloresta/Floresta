# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Pytest configuration and fixtures for node testing.

This module provides fixtures for creating and managing test nodes
(florestad, bitcoind, utreexod) in various configurations.
"""

# pylint: disable=redefined-outer-name

import logging
import os
import time
from typing import List

import pytest
from test_framework import FlorestaTestFramework
from test_framework.constants import (
    WALLET_ADDRESS,
    WALLET_DESCRIPTOR_EXTERNAL,
    WALLET_DESCRIPTOR_INTERNAL,
)
from test_framework.node import Node, NodeType
from test_framework.prepare import get_temp_dir, prepare_binaries
from test_framework.util import Utility


@pytest.fixture(scope="session", autouse=True)
def validate_and_check_environment():
    """Prepare binaries and validate environment before running tests."""
    prepare_binaries()

    temp_dir = get_temp_dir()

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


# pylint: disable=unused-argument
@pytest.hookimpl(tryfirst=True, hookwrapper=True)
def pytest_runtest_makereport(item, call):
    """
    Hook that captures the test result for use in fixtures.
    """
    outcome = yield
    rep = outcome.get_result()
    setattr(item, f"rep_{rep.when}", rep)


@pytest.fixture(scope="function")
def setup_logging(request):
    """
    Configure logging for the test, including the file and line number where the log was called.
    """
    test_name = request.node.name
    logger = logging.getLogger(test_name)

    # Log format to include the file and line
    formatter = logging.Formatter(
        "%(asctime)s - %(levelname)s - %(pathname)s:%(lineno)d - %(message)s"
    )

    # Configure log file
    git_describe = Utility.get_git_describe()
    log_file = os.path.join(get_temp_dir(), "logs", git_describe, f"{test_name}.log")
    os.makedirs(os.path.dirname(log_file), exist_ok=True)
    file_handler = logging.FileHandler(log_file, mode="w")
    file_handler.setFormatter(formatter)

    # Add handlers to the logger
    if not logger.handlers:
        logger.addHandler(file_handler)

    yield logger

    # Capture test result and log it
    if hasattr(request.node, "rep_call") and request.node.rep_call.failed:
        logger.error("=" * 80)
        logger.error("TEST FAILED: %s", test_name)
        logger.error("=" * 80)
        logger.error("%s", request.node.rep_call.longrepr)
        logger.error("=" * 80)

        print(f"📋 Log file: {log_file}\n")

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
            f"--miningaddr={WALLET_ADDRESS}",
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
    node_manager.connect_nodes(florestad_node, utreexod_node)

    return florestad_node, utreexod_node


@pytest.fixture
def florestad_bitcoind(
    florestad_node, bitcoind_node, node_manager
) -> tuple[Node, Node]:
    """
    Creates and starts a `florestad` node and a `bitcoind` node.
    The nodes are automatically connected to each other and are ready for testing.
    """
    node_manager.connect_nodes(florestad_node, bitcoind_node)

    return florestad_node, bitcoind_node


@pytest.fixture
def florestad_bitcoind_utreexod_with_chain(
    florestad_node, bitcoind_node, utreexod_node, node_manager
) -> tuple[Node, Node, Node]:
    """
    Factory fixture that initializes a three-node network with a populated blockchain.

    Instantiates florestad, bitcoind, and utreexod nodes with pre-generated blocks and
    establishes mesh connectivity. Florestad loads wallet descriptors before chain sync,
    allowing it to track transactions during synchronization.
    """

    def _create_nodes_with_chain(
        blocks: int = 100,
        floresta_descriptors: List[str] | None = None,
        addr_coinbase: str = None,
    ) -> tuple[Node, Node, Node]:
        if floresta_descriptors is None:
            floresta_descriptors = [
                WALLET_DESCRIPTOR_EXTERNAL,
                WALLET_DESCRIPTOR_INTERNAL,
            ]

        for descriptor in floresta_descriptors:
            florestad_node.rpc.load_descriptor(descriptor)

        if addr_coinbase:
            bitcoind_node.rpc.generatetoaddress(blocks, addr_coinbase)
        else:
            utreexod_node.rpc.generate(blocks)

        node_manager.connect_nodes(florestad_node, utreexod_node)
        time.sleep(3)
        node_manager.connect_nodes(bitcoind_node, utreexod_node)
        time.sleep(1)
        node_manager.connect_nodes(florestad_node, bitcoind_node)

        return florestad_node, bitcoind_node, utreexod_node

    return _create_nodes_with_chain


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
