"""
bitcoin-test.py

This is an example of how a tests with bitcoin should look like,
see `tests/test_framework/test_framework.py` for more info.
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
import pytest

from conftest import TEST_CHAIN, GENESIS_BLOCK_BLOCK, GENESIS_BLOCK_DIFFICULTY_FLOAT


@pytest.mark.example
def test_bitcoind(bitcoind_node):
    """This test demonstrates how to set up/run a bitcoind"""
    response = bitcoind_node.rpc.get_blockchain_info()

    assert response["chain"] == TEST_CHAIN
    assert response["bestblockhash"] == GENESIS_BLOCK_BLOCK
    assert response["difficulty"] == GENESIS_BLOCK_DIFFICULTY_FLOAT
