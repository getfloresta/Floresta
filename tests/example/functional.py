"""
functional-test.py

This is an example of how a functional-test should look like,
see `tests/test_framework/test_framework.py` for more info.
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
import pytest

from conftest import (
    GENESIS_BLOCK_HEIGHT,
    GENESIS_BLOCK_BLOCK,
    GENESIS_BLOCK_DIFFICULTY_INT,
    GENESIS_BLOCK_LEAF_COUNT,
)


@pytest.mark.example
def test_functional(florestad_node):
    """
    This test demonstrates how to set up and run a `florestad_node`
    and verifies that the blockchain information returned by the node's RPC
    matches the expected values for the genesis block.
    """
    response = florestad_node.rpc.get_blockchain_info()

    assert response["height"] == GENESIS_BLOCK_HEIGHT
    assert response["best_block"] == GENESIS_BLOCK_BLOCK
    assert response["difficulty"] == GENESIS_BLOCK_DIFFICULTY_INT
    assert response["leaf_count"] == GENESIS_BLOCK_LEAF_COUNT
