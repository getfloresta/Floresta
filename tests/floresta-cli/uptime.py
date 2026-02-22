"""
floresta_cli_uptime.py

This functional test cli utility to interact with a Floresta node with `uptime`
"""

import time
import pytest


@pytest.mark.rpc
def test_uptime(florestad_node):
    """Test uptime of a Floresta node using the rpc."""
    florestad = florestad_node

    result = florestad.rpc.uptime()
    assert result is not None
    assert result >= 0

    sleep_time = 5
    expected_min_uptime = result + sleep_time
    expected_max_uptime = result + (sleep_time + 2)

    time.sleep(sleep_time)

    result = florestad.rpc.uptime()
    assert result is not None
    assert expected_min_uptime <= result <= expected_max_uptime
