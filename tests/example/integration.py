"""
integration-test.py

This is an example of how a tests with integrated electrum should look like,
see `tests/test_framework/test_framework.py` for more info.
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
import pytest

from conftest import TEST_CHAIN


@pytest.mark.example
def test_integration(florestad_node, utreexod_node, bitcoind_node):
    """
    This test demonstrates how to set up and run an integration test
    with multiple nodes (`florestad_node`, `utreexod_node`, and `bitcoind_node`).
    """
    floresta_response = florestad_node.rpc.get_blockchain_info()
    utreexo_response = utreexod_node.rpc.get_blockchain_info()
    bitcoin_response = bitcoind_node.rpc.get_blockchain_info()

    assert floresta_response["chain"] == TEST_CHAIN
    assert utreexo_response["chain"] == TEST_CHAIN
    assert bitcoin_response["chain"] == TEST_CHAIN
