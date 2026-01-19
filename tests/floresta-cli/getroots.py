"""
floresta_cli_getroots.py

This functional test cli utility to interact with a Floresta node with `getroots`
"""

import pytest


@pytest.mark.rpc
def test_get_roots(florestad_node):
    """
    Test the `get_roots` RPC method.
    """
    florestad = florestad_node

    vec_hashes = florestad.rpc.get_roots()
    assert len(vec_hashes) == 0
