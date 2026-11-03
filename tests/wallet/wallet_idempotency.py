# SPDX-License-Identifier: MIT OR Apache-2.0

"""
wallet_idempotency.py

This functional test checks that handing the wallet blocks it already processed
does not credit them twice, driving the replay through `rescanblockchain`.
"""

import pytest

from test_framework.constants import WALLET_ADDRESS, WALLET_DESCRIPTOR_EXTERNAL
from test_framework.electrum.client import ElectrumClient

BLOCKS = 20


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
    # balance showing up proves the filters are in place, which is what makes the
    # second half of this test meaningful instead of vacuously green.
    log.info("Loading the wallet descriptor and waiting for the filter rescan...")
    florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)

    balance, utxo_count = florestad.electrum.wait_for_wallet_funded(script_hash)
    log.info(f"Wallet found {utxo_count} utxos worth {balance} sats")

    # Rescanning the same range hands the wallet blocks it already processed.
    log.info("Rescanning the very same range...")
    assert florestad.rpc.perform_request("rescanblockchain", [1, BLOCKS]) is True

    state = (balance, utxo_count)
    assert florestad.electrum.wait_for_wallet_change(script_hash, state) == state
