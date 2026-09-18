# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_stop.py

This functional test cli utility to interact with a Floresta node with `stop`
"""

import pytest


@pytest.mark.rpc
def test_stop(florestad_node):
    """Test stopping a Floresta node using the rpc."""

    florestad_node.rpc.stop()

    # `stop` returns as soon as the daemon accepts the request, so wait on the
    # process. Shutdown can take several seconds when the chainstore is flushed.
    florestad_node.daemon.process.wait(timeout=florestad_node.rpc.TIMEOUT)

    florestad_node.rpc.wait_on_socket(opened=False)
