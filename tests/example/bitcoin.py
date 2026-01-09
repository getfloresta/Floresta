"""
bitcoin.py

Example functional test demonstrating how to use the Floresta test framework
to start and interact with a bitcoind node.

This file shows:
- How to use pytest fixtures provided by tests/conftest.py (e.g. `bitcoind_node`).
- How to call RPC methods via `node.rpc` and assert returned values.
- A minimal example asserting chain name, genesis bestblockhash and difficulty.
"""

import pytest

from conftest import TEST_CHAIN, GENESIS_BLOCK_BLOCK, GENESIS_BLOCK_DIFFICULTY_FLOAT


@pytest.mark.example
def test_bitcoind(bitcoind_node):
    """This test demonstrates how to set up/run a bitcoind"""
    response = bitcoind_node.rpc.get_blockchain_info()

    assert response["chain"] == TEST_CHAIN
    assert response["bestblockhash"] == GENESIS_BLOCK_BLOCK
    assert response["difficulty"] == GENESIS_BLOCK_DIFFICULTY_FLOAT
