# SPDX-License-Identifier: MIT OR Apache-2.0

"""
findtxout.py

This functional test cli utility to interact with a Floresta node with `findtxout` command.
"""

import pytest
from test_framework.constants import (
    JSONRPC_ERRCODE_NO_BLOCK_FILTERS,
    JSONRPC_ERRMSG_NO_BLOCK_FILTERS,
)
from test_framework.util import compare_fields, wait_until

BLOCKS = 20

# Utreexod pays every coinbase to the same address, so the scan below ends up
# caching all of them. These are mined elsewhere to leave one output in the chain
# that our wallet never learns about.
UNCACHED_BLOCKS = 2

# A vout that no coinbase transaction has, used to check the "block was found,
# but the output isn't there" path.
MISSING_VOUT = 99


def find_coinbase_output(florestad, bitcoind, height: int) -> tuple[str, str, dict]:
    """
    Return the `(txid, scriptPubKey hex, gettxout result)` of the coinbase
    output of the block at `height`, using bitcoind as the reference node.

    Both `bestblock` and `confirmations` are relative to the chain tip, so
    the nodes must agree on it for the reference to be comparable. Nothing is
    mined while the test runs, which keeps that true for every later call.
    """
    tip = florestad.rpc.get_bestblockhash()
    assert tip == bitcoind.rpc.get_bestblockhash(), "Nodes disagree on the chain tip."

    block_hash = florestad.rpc.get_blockhash(height)
    txid = florestad.rpc.get_block(block_hash)["tx"][0]

    reference = bitcoind.rpc.get_txout(txid, vout=0, include_mempool=False)
    assert reference is not None, f"Txout for tx {txid} is None in Bitcoind."

    # Pin what the tip-relative fields are expected to hold, so a mismatch
    # points at the value itself instead of at a node lagging behind.
    assert reference["bestblock"] == tip
    assert reference["confirmations"] == florestad.rpc.get_block_count() - height + 1

    return txid, reference["scriptPubKey"]["hex"], reference


# pylint: disable=too-many-locals
@pytest.mark.rpc
def test_find_txout(
    setup_logging, florestad_bitcoind_utreexod_with_filters, node_manager
):
    """
    Test the `findtxout` command for outputs that our wallet doesn't track.

    No descriptor is loaded, so florestad's wallet is empty and `findtxout`
    must fall back to the compact block filters to locate the output. Once
    found, the output is cached, so `gettxout` and any further `findtxout`
    call must answer with the very same structure.
    """
    log = setup_logging
    florestad, bitcoind, utreexod = florestad_bitcoind_utreexod_with_filters(
        BLOCKS, floresta_descriptors=[]
    )

    log.info("Waiting for Floresta and Bitcoind to sync with Utreexod...")
    node_manager.wait_for_sync_nodes()

    log.info(f"Mining {UNCACHED_BLOCKS} blocks paying someone we don't watch...")
    bitcoind.rpc.generate_block(UNCACHED_BLOCKS)
    node_manager.wait_for_sync_nodes()

    height = BLOCKS // 2
    txid, script, reference = find_coinbase_output(florestad, bitcoind, height)

    # Left alone until the very end: asking for it now would cache it too.
    uncached_txid, uncached_script, _ = find_coinbase_output(
        florestad, bitcoind, BLOCKS + 1
    )

    log.info(f"Floresta shouldn't know anything about {txid}:0 yet...")
    assert florestad.rpc.get_txout(txid, vout=0, include_mempool=False) is None

    # The height is only a hint: filters are sought in 50k block strides, so
    # anything below that mark is scanned from the genesis anyway. Hinting a
    # height past the output must still find it.
    #
    # Filters are also downloaded in background after the IBD, so the first
    # lookups may legitimately find nothing until they arrive.
    log.info(f"Searching {txid}:0 on the filters, hinting height {BLOCKS}...")
    found = {}

    def scanned_filters() -> bool:
        """Try to find the output, keeping the last answer around."""
        found["txout"] = florestad.rpc.find_txout(txid, 0, script, BLOCKS)
        return found["txout"] is not None

    wait_until(
        scanned_filters,
        timeout=60,
        error_msg=f"findtxout didn't find {txid}:0",
    )

    compare_fields(found["txout"], reference)

    log.info("The scanned output should now be cached on the wallet...")
    txout_cached = florestad.rpc.get_txout(txid, vout=0, include_mempool=False)
    assert txout_cached is not None, f"Txout for tx {txid} wasn't cached in Floresta."
    compare_fields(txout_cached, reference)

    log.info(f"An output that doesn't exist on {txid} shouldn't be found...")
    assert florestad.rpc.find_txout(txid, MISSING_VOUT, script, height) is None

    # Asking again would find it either way, from the wallet or by scanning the
    # filters a second time, so take the filters away: whatever answers now can
    # only be the wallet. The lookup for them happens after the wallet is checked,
    # which is what we're relying on.
    log.info("Restarting Floresta without the filters to fall back on...")
    florestad.stop()
    florestad.daemon.set_extra_args(["--no-cfilters"])
    florestad.start()

    # Back on its feet before we ask: `find_tx_out` bails out on the IBD before it
    # ever reaches for the filters, and it's the filters we want it to answer about.
    node_manager.connect_nodes(florestad, utreexod)
    node_manager.wait_for_sync_nodes()

    log.info(f"An output we never cached, {uncached_txid}:0, is now unanswerable...")
    florestad.rpc.ensure_rpc_call_error(
        method="findtxout",
        params=[uncached_txid, 0, uncached_script, height],
        expected_status_code=503,
        expected_rpcerror_code=JSONRPC_ERRCODE_NO_BLOCK_FILTERS,
        expected_message=JSONRPC_ERRMSG_NO_BLOCK_FILTERS,
    )

    log.info("...while the output we cached still comes back, from the wallet")
    compare_fields(florestad.rpc.find_txout(txid, 0, script, height), reference)
