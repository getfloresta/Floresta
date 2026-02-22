"""
florestad/tls-test.py

This functional test tests the proper creation of a TLS port on florestad.
"""

import pytest

from test_framework.node import NodeType


@pytest.mark.florestad
def test_tls(add_node_with_tls):
    """
    Test initialization florestad with TLS and test Electrum client connection.
    """
    florestad = add_node_with_tls(NodeType.FLORESTAD)

    assert florestad.electrum.tls

    response = florestad.electrum.ping()
    assert response["result"] is None
    assert response["id"] == 0
    assert response["jsonrpc"] == "2.0"
