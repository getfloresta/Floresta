# SPDX-License-Identifier: MIT OR Apache-2.0

"""
rescanblockchain.py

This functional test exercises the `rescanblockchain` RPC command, checking that
rescanning a range the wallet already processed does not credit it twice.
"""

import hashlib
import time

import pytest

from test_framework.constants import WALLET_ADDRESS, WALLET_DESCRIPTOR_EXTERNAL
from test_framework.node import NodeType

BLOCKS = 20

# Both `loaddescriptor` and `rescanblockchain` answer as soon as they spawn the
# rescan task, so the wallet has to be polled until the rescan lands.
POLL_INTERVAL = 1
RESCAN_TIMEOUT = 60

# How long we let a rescan of already known blocks settle before deciding the
# wallet was left alone.
SETTLE_TIMEOUT = 30


def electrum_script_hash(script_pubkey: str) -> str:
    """
    Build the Electrum script hash of a scriptPubKey: its sha256, byte reversed.
    """
    return hashlib.sha256(bytes.fromhex(script_pubkey)).digest()[::-1].hex()


def wallet_state(node, script_hash) -> tuple:
    """
    Read the confirmed balance and the utxo count the wallet holds for a script hash.

    The balance is `None` while the address is not cached by the wallet.
    """
    balance = node.electrum.get_balance(script_hash)["result"]["confirmed"]
    utxos = node.electrum.list_unspent(script_hash)["result"]
    return balance, len(utxos)


def wait_until_funded(node, script_hash) -> tuple:
    """
    Poll the wallet until it holds a positive balance, or the timeout expires.
    """
    deadline = time.time() + RESCAN_TIMEOUT
    while time.time() < deadline:
        state = wallet_state(node, script_hash)
        if state[0]:
            return state
        time.sleep(POLL_INTERVAL)

    return wallet_state(node, script_hash)


def wait_for_change(node, script_hash, state) -> tuple:
    """
    Poll the wallet until its state moves away from `state`, or the timeout expires.

    Returns `state` untouched when nothing ever moved.
    """
    deadline = time.time() + SETTLE_TIMEOUT
    while time.time() < deadline:
        current = wallet_state(node, script_hash)
        if current != state:
            return current
        time.sleep(POLL_INTERVAL)

    return state


@pytest.mark.rpc
def test_rescanblockchain_does_not_double_count(setup_logging, node_manager):
    """
    Rescanning blocks the wallet already knows must leave its balance untouched.
    """
    log = setup_logging

    florestad = node_manager.add_node_default_args(variant=NodeType.FLORESTAD)
    node_manager.run_node(florestad)

    # bitcoind is here only to serve compact block filters: floresta downloads
    # them from a peer, and every rescan path needs them to find our blocks.
    bitcoind = node_manager.add_node_extra_args(
        variant=NodeType.BITCOIND,
        extra_args=["-blockfilterindex=1", "-peerblockfilters=1"],
    )
    node_manager.run_node(bitcoind)

    # utreexod mines every block to WALLET_ADDRESS and serves the utreexo proofs
    # floresta needs in order to sync.
    utreexod = node_manager.add_node_extra_args(
        variant=NodeType.UTREEXOD,
        extra_args=[
            f"--miningaddr={WALLET_ADDRESS}",
            "--utreexoproofindex",
            "--prune=0",
        ],
    )
    node_manager.run_node(utreexod)

    utreexod.rpc.generate(BLOCKS)

    node_manager.connect_nodes(florestad, utreexod)
    time.sleep(3)
    node_manager.connect_nodes(bitcoind, utreexod)
    time.sleep(1)
    node_manager.connect_nodes(florestad, bitcoind)

    log.info("Waiting for the nodes to sync...")
    node_manager.wait_for_sync_nodes()

    script_pubkey = bitcoind.rpc.perform_request("validateaddress", [WALLET_ADDRESS])[
        "scriptPubKey"
    ]
    script_hash = electrum_script_hash(script_pubkey)

    # No descriptor was loaded, so the wallet ignored every block it just saw.
    assert wallet_state(florestad, script_hash) == (None, 0)

    # Loading the descriptor kicks off a rescan driven by the block filters. The
    # balance showing up proves the filters are in place, which is what makes the
    # second half of this test meaningful instead of vacuously green.
    log.info("Loading the wallet descriptor and waiting for the filter rescan...")
    florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)

    state = wait_until_funded(florestad, script_hash)
    balance, utxo_count = state
    assert balance, "the filter rescan never credited the wallet"
    log.info(f"Wallet found {utxo_count} utxos worth {balance} sats")

    # Rescanning the same range hands the wallet blocks it already processed.
    log.info("Rescanning the very same range...")
    assert florestad.rpc.perform_request("rescanblockchain", [1, BLOCKS]) is True

    assert wait_for_change(florestad, script_hash, state) == state
