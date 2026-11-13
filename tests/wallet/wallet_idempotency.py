# SPDX-License-Identifier: MIT OR Apache-2.0

"""
wallet_idempotency.py

This functional test checks that handing the wallet blocks it already processed
does not credit them twice, driving the replay through `rescanblockchain`.
"""

import pytest

from test_framework.constants import WALLET_ADDRESS, WALLET_DESCRIPTOR_EXTERNAL
from test_framework.electrum.client import ElectrumClient
from test_framework.util import wait_until

BLOCKS = 20
COINBASE_SATS = 5_000_000_000
SETTLED_STATE = (BLOCKS * COINBASE_SATS, BLOCKS)


@pytest.mark.wallet
def test_rescanblockchain_does_not_double_count(
    setup_logging, florestad_bitcoind_utreexod_with_filters
):
    """
    Rescanning blocks the wallet already knows must leave its balance untouched.
    """
    log = setup_logging
    florestad, bitcoind, _ = florestad_bitcoind_utreexod_with_filters(BLOCKS)

    script_pubkey = bitcoind.rpc.perform_request("validateaddress", [WALLET_ADDRESS])[
        "scriptPubKey"
    ]
    script_hash = ElectrumClient.script_hash(script_pubkey)

    # No descriptor was loaded, so the wallet ignored every block it just saw.
    assert florestad.electrum.wallet_state(script_hash) == (None, 0)

    # Loading the descriptor kicks off a rescan driven by the block filters. The
    # state only settles once every block was credited, which proves the filters
    # are in place and makes the second half of this test meaningful.
    log.info("Loading the wallet descriptor and waiting for the filter rescan...")
    florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)

    florestad.electrum.wait_for_wallet_state(script_hash, SETTLED_STATE)

    # Rescanning the same range hands the wallet blocks it already processed.
    log.info("Rescanning the very same range...")
    assert florestad.rpc.perform_request("rescanblockchain", [1, BLOCKS]) is True

    # `rescanblockchain` returns as soon as it spawns the rescan, so wait for both
    # rescans to report they are done before reading the final state.
    wait_until(
        lambda: florestad.count_log_lines("rescan finished") == 2,
        timeout=60,
        error_msg="rescans never finished",
    )

    assert florestad.electrum.wallet_state(script_hash) == SETTLED_STATE
