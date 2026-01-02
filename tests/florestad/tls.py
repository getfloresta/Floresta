"""
florestad/tls-test.py

This functional test tests the proper creation of a TLS port on florestad.
"""

import pytest

from test_framework.electrum.client import ElectrumClient


@pytest.mark.florestad
def test_tls(add_node_with_tls):
    """
    Test initialization florestad with TLS and test Electrum client connection.
    """
    florestad = add_node_with_tls("florestad")

    assert florestad.get_port("electrum-server-tls") is not None

    electrum = ElectrumClient(
        florestad.get_host(),
        florestad.get_port("electrum-server-tls"),
        tls=True,
    )

    assert electrum is not None

    response = electrum.ping()
    assert response["result"] is None
    assert response["id"] == 0
    assert response["jsonrpc"] == "2.0"
