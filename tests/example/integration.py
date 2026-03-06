"""
integration.py

Example integration test that demonstrates how to run a multi-node scenario
with Florestad, Utreexod and Bitcoind. This file shows:

- How to use pytest fixtures provided by tests/conftest.py (for example
  `florestad_node`, `utreexod_node`, and `bitcoind_node`) to create, configure
  and teardown multiple node instances for a single test.
- How to call RPC methods via `node.rpc` for each node and perform cross-node
  assertions to ensure interoperability and consistent chain state.
"""

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
