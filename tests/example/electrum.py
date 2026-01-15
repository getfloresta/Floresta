"""
electrum-test.py

This is an example of how a tests with integrated electrum should look like,
see `tests/test_framework/test_framework.py` for more info.
"""

import pytest

from test_framework.electrum.client import ElectrumClient

EXPECTED_VERSION = ["Floresta 0.4.0", "1.4"]


@pytest.mark.example
def test_electrum(setup_logging, florestad_node):
    """
    This test demonstrates how to set up and run an Electrum client,
    and verifies that the Electrum server responds with the expected version.
    """
    host = florestad_node.get_host()
    port = florestad_node.get_port("electrum-server")
    electrum = ElectrumClient(setup_logging, host, port)

    rpc_response = electrum.get_version()

    assert rpc_response["result"][0] == EXPECTED_VERSION[0]
    assert rpc_response["result"][1] == EXPECTED_VERSION[1]
