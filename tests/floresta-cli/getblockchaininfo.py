# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getblockchaininfo.py

Functional test for `getblockchaininfo`. Mines blocks via utreexod, then
compares florestad's response against bitcoind's field-by-field. Bitcoind is
treated as the source of truth, so future Core API changes surface as test
failures rather than silent drift.
"""

import time
import pytest

from test_framework.util import wait_for_chain_sync

TIMEOUT_SECONDS = 30
MINE_BLOCKS = 10
EXTRA_BLOCKS = 5
# Fields where florestad diverges from bitcoind:
#   - `pruned` is always True (Utreexo discards full blocks)
#   - `size_on_disk` reports mmap capacity, not blk*.dat size; checked below.
FLORESTA_SPECIFIC_FIELDS = ("pruned", "size_on_disk")


def _wait_for_size_growth(node, baseline):
    end = time.time() + TIMEOUT_SECONDS
    current = baseline
    while time.time() < end:
        current = node.rpc.get_blockchain_info()["size_on_disk"]
        if current > baseline:
            return current
        time.sleep(0.5)
    return current


@pytest.mark.rpc
def test_get_blockchain_info(florestad_bitcoind_utreexod_with_chain):
    """
    Compare florestad's getblockchaininfo response against bitcoind's after a
    small chain extension. Iterates bitcoind's keys so any new field added in
    a future Core release fails the test until florestad implements it.
    """
    florestad, bitcoind, utreexod = florestad_bitcoind_utreexod_with_chain(MINE_BLOCKS)

    wait_for_chain_sync(
        florestad,
        bitcoind,
        utreexod,
        MINE_BLOCKS,
        TIMEOUT_SECONDS,
        wait_for_ibd_exit=True,
    )

    floresta_info = florestad.rpc.get_blockchain_info()
    bitcoind_info = bitcoind.rpc.get_blockchain_info()

    for key, bval in bitcoind_info.items():
        if key in FLORESTA_SPECIFIC_FIELDS:
            continue
        fval = floresta_info[key]
        if key == "difficulty":
            # Allow float rounding noise.
            assert round(fval, 3) == round(bval, 3)
        else:
            assert fval == bval, f"{key}: floresta={fval} bitcoind={bval}"

    # size_on_disk: well-formed, grows after mining.
    size_before = floresta_info["size_on_disk"]
    assert isinstance(size_before, int)

    utreexod.rpc.generate(EXTRA_BLOCKS)
    # Poll for growth: get_block_count moves on header receipt, but acc roots
    # are only written post-validation.
    size_after = _wait_for_size_growth(florestad, size_before)
    assert size_after > size_before, (
        f"size_on_disk did not grow after mining {EXTRA_BLOCKS} blocks: "
        f"before={size_before} after={size_after}"
    )
