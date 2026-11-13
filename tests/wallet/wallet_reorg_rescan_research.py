# SPDX-License-Identifier: MIT OR Apache-2.0
"""Research-only regression for Floresta issue #1451.

Demonstrate that rescanning the unchanged funding blocks after an orphaned
confirmed spend does not repair the watch-only wallet, including after restart.

Expected to FAIL on Floresta PR #1443 (24a8c77), until a wallet reorg fix lands.
Reproduced on local regtest; independent human review is required.
"""

from hashlib import sha256

import pytest

from test_framework.constants import (
    WALLET_ADDRESS,
    WALLET_DESCRIPTOR_EXTERNAL,
    WALLET_DESCRIPTOR_PRIV_EXTERNAL,
)
from test_framework.node import NodeType
from test_framework.util import wait_until

MATURE_BLOCKS = 105
COINBASE_SATS = 5_000_000_000
FORK_ADDRESS = "bcrt1q3ml87jemlfvk7lq8gfs7pthvj5678ndnxnw9ch"
EXPECTED_WALLET_STATE = (MATURE_BLOCKS * COINBASE_SATS, MATURE_BLOCKS)


def wallet_outpoints(florestad, script_hash):
    """Return the exact unspent outpoints for one watched script."""
    return {
        (entry["tx_hash"], entry["tx_pos"])
        for entry in florestad.electrum.list_unspent(script_hash)
    }


def wallet_spend_heights(florestad, script_hash, spend_txid):
    """Get the cached history heights of a transaction if it is still known."""
    return [
        entry["height"]
        for entry in florestad.electrum.get_history(script_hash)
        if entry["tx_hash"] == spend_txid
    ]


def assert_chain_at(florestad, tip_hash, height):
    """Require the same header tip and validated height."""
    info = florestad.rpc.get_blockchain_info()
    assert info["bestblockhash"] == tip_hash, info
    assert info["blocks"] == height, info
    assert info["headers"] == height, info


def run_complete_rescan(florestad, start_height, stop_height, log, label):
    """Wait for both filter lookup and rescan completion, not just RPC acceptance.

    A completed rescan does not itself prove every requested block was fetched;
    the full watched-UTXO baseline established before the reorg is the control
    that confirms the funding blocks were previously discoverable by filters.
    """
    before_finished = florestad.count_log_lines("rescan finished")
    before_hits = florestad.count_log_lines("rescan filter hits:")

    assert (
        florestad.rpc.perform_request("rescanblockchain", [start_height, stop_height])
        is True
    )

    wait_until(
        lambda: florestad.count_log_lines("rescan finished") > before_finished,
        timeout=180,
        interval=1,
        error_msg=f"{label}: rescan never finished",
    )

    assert (
        florestad.count_log_lines("rescan filter hits:") > before_hits
    ), f"{label}: rescan did not report successful filter matching"
    log.info("RESCAN_COMPLETED label=%s range=%s..%s", label, start_height, stop_height)


# Complex end-to-end reorg scenario with sequential state verification.
# pylint: disable=too-many-locals,too-many-statements
@pytest.mark.expensive
def test_reorg_rescan_restores_orphaned_spend(
    setup_logging, florestad_bitcoind_utreexod_with_filters, node_manager
):
    """Reorg away a confirmed spend, rescan funding blocks, inspect persistence."""
    log = setup_logging

    # Unlike the default three-node fixture, this upstream fixture enables
    # Bitcoin Core's BIP157 filter index and Floresta's filter download path.
    florestad, bitcoind, source = florestad_bitcoind_utreexod_with_filters(
        MATURE_BLOCKS
    )
    common_hash = source.rpc.get_blockhash(MATURE_BLOCKS)
    wait_until(
        lambda: florestad.count_log_lines("starting running node...") >= 1,
        timeout=120,
        interval=1,
        error_msg="Floresta never reached running mode",
    )
    assert_chain_at(florestad, common_hash, MATURE_BLOCKS)

    assert FORK_ADDRESS != WALLET_ADDRESS
    assert bitcoind.rpc.perform_request("validateaddress", [FORK_ADDRESS])["isvalid"]
    script_pubkey = bitcoind.rpc.perform_request("validateaddress", [WALLET_ADDRESS])[
        "scriptPubKey"
    ]
    script_hash = sha256(bytes.fromhex(script_pubkey)).digest()[::-1].hex()

    # Descriptor is loaded only AFTER all 105 funding blocks have synced.
    # Thus the full initial balance verifies the compact-filter rescan path.
    assert florestad.electrum.wallet_state(script_hash) == (None, 0)
    assert florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL) is True
    florestad.electrum.wait_for_wallet_state(
        script_hash, EXPECTED_WALLET_STATE, timeout=180
    )
    wait_until(
        lambda: florestad.count_log_lines("rescan finished") >= 1,
        timeout=180,
        interval=1,
        error_msg="Initial descriptor filter scan never finished",
    )
    original_outpoints = wallet_outpoints(florestad, script_hash)
    assert len(original_outpoints) == MATURE_BLOCKS
    log.info("FILTER_BASELINE_OK utxos=%d", len(original_outpoints))

    # Verify that explicit replay of all original funding blocks is safe.
    run_complete_rescan(florestad, 1, MATURE_BLOCKS, log, "pre_reorg_control")
    assert wallet_outpoints(florestad, script_hash) == original_outpoints
    assert florestad.electrum.wallet_state(script_hash) == EXPECTED_WALLET_STATE
    log.info("RESCAN_CONTROL_OK")

    # Construct a fork using only raw block submission. Never put the original
    # spend into this miner's mempool or make it peer with the source chain.
    fork = node_manager.add_node_extra_args(
        variant=NodeType.UTREEXOD,
        extra_args=[
            f"--miningaddr={FORK_ADDRESS}",
            "--utreexoproofindex",
            "--prune=0",
            "--blocksonly",
            "--connect=127.0.0.1:1",
            "--nodnsseed",
        ],
    )
    node_manager.run_node(fork)
    assert fork.rpc.get_block_count() == 0
    assert not fork.rpc.get_peerinfo()

    for height in range(1, MATURE_BLOCKS + 1):
        block_hash = source.rpc.get_blockhash(height)
        raw_block = source.rpc.perform_request("getblock", [block_hash, 0])
        assert isinstance(raw_block, str) and raw_block
        result = fork.rpc.perform_request("submitblock", [raw_block])
        assert result is None, f"Fork rejected common block {height}: {result}"
        assert fork.rpc.get_blockhash(height) == block_hash

    assert fork.rpc.get_block_count() == MATURE_BLOCKS
    assert fork.rpc.get_blockhash(MATURE_BLOCKS) == common_hash
    assert not fork.rpc.get_peerinfo()
    assert not fork.rpc.perform_request("getrawmempool", [])
    log.info("FORK_PREFIX_IMPORTED height=%d hash=%s", MATURE_BLOCKS, common_hash)

    # Spend a genuine, mature watched output with repository-owned regtest keys.
    bitcoind.rpc.create_wallet("reorg_rescan_spender")
    assert WALLET_ADDRESS in bitcoind.rpc.perform_request(
        "deriveaddresses", [WALLET_DESCRIPTOR_PRIV_EXTERNAL, [0, 0]]
    )
    imported = bitcoind.rpc.perform_request(
        "importdescriptors",
        [
            [
                {
                    "desc": WALLET_DESCRIPTOR_PRIV_EXTERNAL,
                    "timestamp": 0,
                    "range": [0, 0],
                    "active": False,
                }
            ]
        ],
    )
    assert len(imported) == 1 and imported[0].get("success"), imported
    spendable = [
        coin
        for coin in bitcoind.rpc.perform_request(
            "listunspent", [1, 9_999_999, [WALLET_ADDRESS]]
        )
        if coin.get("spendable")
    ]
    assert spendable, "Bitcoin Core has no mature watched output to spend"

    txid = bitcoind.rpc.perform_request(
        "sendtoaddress",
        [FORK_ADDRESS, 1.0, None, None, False, None, None, "unset", None, 2.0],
    )
    tx = bitcoind.rpc.perform_request("getrawtransaction", [txid, True])
    spent = {(txin["txid"], txin["vout"]) for txin in tx["vin"]} & original_outpoints
    assert spent, "The transaction did not spend a watched UTXO"

    wait_until(
        lambda: txid in source.rpc.perform_request("getrawmempool", []),
        timeout=60,
        interval=1,
        error_msg="Spending transaction did not reach original miner",
    )
    spend_block = source.rpc.generate(1)[0]
    assert txid in source.rpc.get_block(spend_block, 1)["tx"]
    wait_until(
        lambda: florestad.count_log_lines(
            f"hash={spend_block} height={MATURE_BLOCKS + 1} tx_count="
        )
        >= 1,
        timeout=180,
        interval=1,
        error_msg="Original spending block not validated",
    )
    wait_until(
        lambda: spent.isdisjoint(wallet_outpoints(florestad, script_hash)),
        timeout=60,
        interval=1,
        error_msg="Confirmed spend did not remove its wallet input",
    )
    assert_chain_at(florestad, spend_block, MATURE_BLOCKS + 1)
    assert MATURE_BLOCKS + 1 in wallet_spend_heights(florestad, script_hash, txid)
    log.info("CONFIRMED_SPEND_SETUP_OK txid=%s inputs=%s", txid, sorted(spent))

    # Both original producers must be stopped before joining the fork.
    assert fork.rpc.get_block_count() == MATURE_BLOCKS
    assert not fork.rpc.get_peerinfo()
    assert txid not in fork.rpc.perform_request("getrawmempool", [])
    bitcoind.stop()
    source.stop()

    # Connect BEFORE mining, otherwise Floresta may never learn the competing
    # headers until the stale-tip timer elapses.
    node_manager.connect_nodes(florestad, fork)
    assert florestad.is_peer_connected(fork)
    assert fork.is_peer_connected(florestad)
    log.info("FORK_PEER_CONNECTED_BEFORE_MINING")

    first_replacement = fork.rpc.generate(1)[0]
    assert first_replacement != spend_block

    def first_header_known():
        result = florestad.rpc.noraise_request(
            "getblockheader", [first_replacement, False]
        )
        return result["status_code"] == 200 and isinstance(
            result["body"].get("result"), str
        )

    wait_until(
        first_header_known,
        timeout=60,
        interval=1,
        error_msg="Floresta did not receive first replacement header",
    )
    log.info("FORK_FIRST_HEADER_ACCEPTED hash=%s", first_replacement)

    last_replacement = fork.rpc.generate(1)[0]
    assert fork.rpc.get_block_count() == MATURE_BLOCKS + 2
    assert fork.rpc.get_blockhash(MATURE_BLOCKS) == common_hash
    assert last_replacement != first_replacement
    for block_hash in (first_replacement, last_replacement):
        txids = fork.rpc.get_block(block_hash, 1)["tx"]
        assert len(txids) == 1 and txid not in txids, txids

    wait_until(
        lambda: florestad.count_log_lines(
            f"hash={last_replacement} " f"height={MATURE_BLOCKS + 2} tx_count=1"
        )
        >= 1,
        timeout=180,
        interval=1,
        error_msg="Floresta never validated the replacement branch",
    )
    assert_chain_at(florestad, last_replacement, MATURE_BLOCKS + 2)
    log.info("ORPHANED_SPEND_FORK_OK old=%s new=%s", spend_block, last_replacement)

    def audit():
        present = wallet_outpoints(florestad, script_hash)
        return {
            "missing": sorted(original_outpoints - present),
            "extra": sorted(present - original_outpoints),
            "state": florestad.electrum.wallet_state(script_hash),
            "spend_heights": wallet_spend_heights(florestad, script_hash, txid),
        }

    before_rescan = audit()
    log.info("REORG_AUDIT before_rescan %s", before_rescan)

    # Every original funding block was already matched by the descriptor
    # control. Replay exactly these SAME blocks after the reorganization.
    run_complete_rescan(florestad, 1, MATURE_BLOCKS, log, "after_reorg")
    assert_chain_at(florestad, last_replacement, MATURE_BLOCKS + 2)
    after_rescan = audit()
    log.info("RESCAN_AUDIT after_rescan %s", after_rescan)

    # Collect restart evidence even when the current implementation is broken.
    # The assertions are deliberately deferred until both audits have logged.
    florestad.stop()
    if florestad.electrum.conn is not None:
        florestad.electrum.conn.close()
        florestad.electrum.conn = None
    florestad.start()
    assert_chain_at(florestad, last_replacement, MATURE_BLOCKS + 2)
    after_restart = audit()
    log.info("RESTART_AUDIT after_rescan_and_restart %s", after_restart)

    for stage, result in (
        ("after rescan", after_rescan),
        ("after rescan and restart", after_restart),
    ):
        assert not result["missing"], f"Missing UTXOs {stage}: {result}"
        assert not result["extra"], f"Unexpected UTXOs {stage}: {result}"
        assert (
            result["state"] == EXPECTED_WALLET_STATE
        ), f"Incorrect balance or UTXO count {stage}: {result}"
        assert all(
            height <= 0 for height in result["spend_heights"]
        ), f"Orphaned spend remains confirmed {stage}: {result}"

    log.info("ORPHANED_SPEND_RESCAN_RECOVERY_OK")
