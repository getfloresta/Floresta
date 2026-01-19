"""
floresta_cli_getblockchainfo.py

This functional test cli utility to interact with a Floresta node with `getblockchaininfo`
"""

import pytest


@pytest.mark.rpc
def test_get_blockchain_info(florestad_node):
    """
    Test `getblockchaininfo` with a fresh node and its first block
    """
    florestad = florestad_node

    response = florestad.rpc.get_blockchain_info()
    assert (
        response["best_block"]
        == "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
    )
    assert response["difficulty"] == 1
    assert response["height"] == 0
    assert response["ibd"] is True
    assert response["latest_block_time"] == 1296688602
    assert (
        response["latest_work"]
        == "0000000000000000000000000000000000000000000000000000000000000002"
    )
    assert response["leaf_count"] == 0
    assert response["progress"] == 0
    assert response["root_count"] == 0
    assert response["root_hashes"] == []
    assert response["validated"] == 0
