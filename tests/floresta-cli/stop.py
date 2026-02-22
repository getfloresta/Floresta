"""
floresta_cli_stop.py

This functional test cli utility to interact with a Floresta node with `stop`
"""

import pytest


@pytest.mark.rpc
def test_stop(florestad_node):
    """Test stopping a Floresta node using the rpc."""
    florestad = florestad_node

    florestad.rpc.stop()
    florestad.daemon.process.wait(5)

    florestad.rpc.wait_on_socket(opened=False)
