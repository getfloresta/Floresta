"""
functional.py

Example functional test demonstrating the recommended structure and patterns
for integration tests using the Floresta test framework.

This file shows:
- How to use pytest fixtures provided by tests/conftest.py (for example `florestad_node`)
  to create, configure and teardown a node instance.
- How to call RPC methods via `node.rpc` and assert returned values.
"""

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
