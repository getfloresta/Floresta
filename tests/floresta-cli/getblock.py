"""
floresta_cli_getblock.py

This functional test cli utility to interact with a Floresta node with `getblock`
"""

import pytest

from conftest import (
    GENESIS_BLOCK_BLOCK,
    GENESIS_BLOCK_HEIGHT,
)


@pytest.mark.rpc
def test_get_block(florestad_node):
    """
    Test `getblock` to get the genesis block.
    """
    florestad = florestad_node

    block_serialize = florestad.rpc.get_block(GENESIS_BLOCK_BLOCK, 0)

    assert (
        block_serialize
        == "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b"
        "27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4adae5494dffff7f200200000001010000000100000"
        "00000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d01044554686"
        "52054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f662073656"
        "36f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe55482"
        "71967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7b"
        "a0b8d578a4c702b6bf11d5fac00000000"
    )

    block_verbose = florestad.rpc.get_block(GENESIS_BLOCK_BLOCK, 1)
    assert block_verbose["bits"] == "207fffff"
    assert (
        block_verbose["chainwork"]
        == "0000000000000000000000000000000000000000000000000000000000000002"
    )
    assert block_verbose["confirmations"] == 1
    assert block_verbose["difficulty"] == 4.6565423739069247e-10
    assert block_verbose["hash"] == GENESIS_BLOCK_BLOCK
    assert block_verbose["height"] == GENESIS_BLOCK_HEIGHT
    assert block_verbose["mediantime"] == 1296688602
    assert (
        block_verbose["merkleroot"]
        == "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b"
    )
    assert block_verbose["n_tx"] == 1
    assert block_verbose["nonce"] == 2
    assert (
        block_verbose["previousblockhash"]
        == "0000000000000000000000000000000000000000000000000000000000000000"
    )
    assert block_verbose["size"] == 285
    assert block_verbose["strippedsize"] == 285
    assert block_verbose["time"] == 1296688602
    assert len(block_verbose["tx"]) == 1
    assert (
        block_verbose["tx"][0]
        == "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b"
    )
    assert block_verbose["version"] == 1
    assert block_verbose["versionHex"] == "00000001"
    assert block_verbose["weight"] == 1140
    assert (
        block_verbose["target"]
        == "7fffff0000000000000000000000000000000000000000000000000000000000"
    )
