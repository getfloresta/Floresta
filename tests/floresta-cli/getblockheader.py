"""
floresta_cli_getblockheader.py

This functional test cli utility to interact with a Floresta node with `getblockheader`
"""

import pytest

from conftest import GENESIS_BLOCK_BLOCK


@pytest.mark.rpc
def test_get_block_header(florestad_node):
    """
    Test `getblockheader` to get the genesis block header.
    """
    florestad = florestad_node

    result = florestad.rpc.get_blockheader(GENESIS_BLOCK_BLOCK)

    assert result["version"] == 1
    assert (
        result["prev_blockhash"]
        == "0000000000000000000000000000000000000000000000000000000000000000"
    )
    assert (
        result["merkle_root"]
        == "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b"
    )
    assert result["time"] == 1296688602
    assert result["bits"] == 545259519
    assert result["nonce"] == 2
