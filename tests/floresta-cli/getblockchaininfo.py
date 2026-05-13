# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getblockchaininfo.py

Functional test for `getblockchaininfo`. Mines blocks via utreexod, then
compares florestad's response against bitcoind's field-by-field. Bitcoind is
treated as the source of truth, so future Core API changes surface as test
failures rather than silent drift.
"""

import pytest

from test_framework.util import wait_until

MINE_BLOCKS = 10
EXTRA_BLOCKS = 5
# Fields where florestad diverges from bitcoind:
#   - `pruned` is always True (Utreexo discards full blocks)
#   - `size_on_disk` reports mmap capacity, not blk*.dat size; checked below.
FLORESTA_SPECIFIC_FIELDS = ("pruned", "size_on_disk")


@pytest.mark.rpc
def test_get_blockchain_info(node_manager, florestad_bitcoind_utreexod_with_chain):
    """
    Compare florestad's getblockchaininfo response against bitcoind's after a
    small chain extension. Iterates bitcoind's keys so any new field added in
    a future Core release fails the test until florestad implements it.
    """
    florestad, bitcoind, utreexod = florestad_bitcoind_utreexod_with_chain(MINE_BLOCKS)

    node_manager.wait_for_sync_nodes()

    floresta_info = florestad.rpc.get_blockchain_info()
    bitcoind_info = bitcoind.rpc.get_blockchain_info()

    for key, bval in bitcoind_info.items():
        if key in FLORESTA_SPECIFIC_FIELDS:
            continue
        fval = floresta_info[key]
        if key == "difficulty":
            # Allow float rounding noise.
            assert round(fval, 3) == round(bval, 3)
        elif key == "verificationprogress":
            # Floresta computes time-based progress; bitcoind computes
            # tx-weighted. Both converge at endpoints but drift mid-sync
            # and at the tip (block timestamps lag `now` by seconds).
            assert abs(fval - bval) < 1e-3
        else:
            assert fval == bval, f"{key}: floresta={fval} bitcoind={bval}"

    # size_on_disk: well-formed, grows after mining.
    size_before = floresta_info["size_on_disk"]
    assert isinstance(size_before, int)

    utreexod.rpc.generate(EXTRA_BLOCKS)
    node_manager.wait_for_sync_nodes()

    # Poll for growth: get_block_count moves on header receipt, but acc roots
    # are only written post-validation.
    wait_until(
        lambda: florestad.rpc.get_blockchain_info()["size_on_disk"] > size_before
    )
