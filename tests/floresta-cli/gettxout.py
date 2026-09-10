# SPDX-License-Identifier: MIT OR Apache-2.0

"""
gettxout.py

This functional test cli utility to interact with a Floresta node with `gettxout` command.
"""

import pytest
from test_framework.util import compare_fields


# pylint: disable=too-many-locals
@pytest.mark.rpc
def test_get_txout(setup_logging, florestad_bitcoind_utreexod_with_chain, node_manager):
    """
    Test the `gettxout` command for a specific transaction output.
    """
    log = setup_logging
    blocks = 10
    florestad, bitcoind, _ = florestad_bitcoind_utreexod_with_chain(blocks)

    log.info("Waiting for Floresta and Bitcoind to sync with Utreexod...")
    node_manager.wait_for_sync_nodes()

    # `bestblock` and `confirmations` are relative to the chain tip, so both
    # nodes must agree on it for the comparison below to hold. Nothing is
    # mined while the test runs, which keeps that true for the whole loop.
    log.info("Comparing gettxout results between Floresta and Bitcoind...")
    assert (
        florestad.rpc.get_bestblockhash() == bitcoind.rpc.get_bestblockhash()
    ), "Nodes disagree on the chain tip."

    for height in range(2, blocks):
        block_hash = florestad.rpc.get_blockhash(height)
        block = florestad.rpc.get_block(block_hash)
        log.info(f"Comparing gettxout results for {height} block {block_hash}...")

        for tx in block["tx"]:
            txout_floresta = florestad.rpc.get_txout(tx, vout=0, include_mempool=False)

            assert txout_floresta is not None, f"Txout for tx {tx} is None in Floresta."

            txout_bitcoind = bitcoind.rpc.get_txout(tx, vout=0, include_mempool=False)
            assert txout_bitcoind is not None, f"Txout for tx {tx} is None in Bitcoind."

            compare_fields(txout_floresta, txout_bitcoind)
