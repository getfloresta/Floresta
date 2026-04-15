# SPDX-License-Identifier: MIT OR Apache-2.0

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

from test_framework.constants import (
    GENESIS_BLOCK_HEIGHT,
    GENESIS_BLOCK_HASH,
    GENESIS_BLOCK_DIFFICULTY_FLOAT,
)


@pytest.mark.example
def test_functional(florestad_node):
    """
    This test demonstrates how to set up and run a `florestad_node`
    and verifies that the blockchain information returned by the node's RPC
    matches the expected values for the genesis block.
    """
    response = florestad_node.rpc.get_blockchain_info()

    assert response["blocks"] == GENESIS_BLOCK_HEIGHT
    assert response["bestblockhash"] == GENESIS_BLOCK_HASH
    assert response["difficulty"] == pytest.approx(GENESIS_BLOCK_DIFFICULTY_FLOAT)
