"""
electrum-test.py

This is an example of how a tests with integrated electrum should look like,
see `tests/test_framework/test_framework.py` for more info.
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
import pytest

EXPECTED_VERSION = ["Floresta 0.4.0", "1.4"]


@pytest.mark.example
def test_electrum(florestad_node):
    """
    This test demonstrates how to set up and run an Electrum client,
    and verifies that the Electrum server responds with the expected version.
    """
    rpc_response = florestad_node.electrum.get_version()

    assert rpc_response["result"][0] == EXPECTED_VERSION[0]
    assert rpc_response["result"][1] == EXPECTED_VERSION[1]
