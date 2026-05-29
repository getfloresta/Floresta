# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Test the floresta's `getbestblockhash` after mining a few block with
utreexod. Then, assert that the command returns the same hash of
`best_block` or `bestblockhash` given in `getblockchaininfo` of floresta
and utreexod, respectively.
"""

import pytest


@pytest.mark.rpc
def test_get_best_block_hash(florestad_utreexod, node_manager):
    """
    Test checks if Floresta can synchronize with the blockchain
    and retrieve the hash of the last block via the getbestblockhash RPC.
    """

    florestad, utreexod = florestad_utreexod

    floresta_best_block = florestad.rpc.get_bestblockhash()
    utreexo_best_block = utreexod.rpc.get_blockchain_info()["bestblockhash"]
    assert floresta_best_block == utreexo_best_block

    utreexod.rpc.generate(10)
    node_manager.wait_for_sync(florestad, 10)

    utreexo_chain = utreexod.rpc.get_blockchain_info()
    floresta_best_block = florestad.rpc.get_bestblockhash()

    assert floresta_best_block == utreexo_chain["bestblockhash"]
