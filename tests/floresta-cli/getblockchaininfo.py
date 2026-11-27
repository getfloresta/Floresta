# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getblockchaininfo.py

Functional test for `getblockchaininfo`. Mines blocks via utreexod, then
compares florestad's response against bitcoind's field-by-field. Bitcoind is
treated as the source of truth, so future Core API changes surface as test
failures rather than silent drift.
"""

import pytest

from test_framework.util import wait_until, compare_fields

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

    # `verificationprogress` is time-based here and tx-weighted in bitcoind, so
    # it drifts past the default 1e-8.
    compare_fields(
        floresta_info,
        bitcoind_info,
        ignore_fields=FLORESTA_SPECIFIC_FIELDS,
        float_tol=1e-3,
    )

    # size_on_disk: well-formed, grows after mining.
    size_before = floresta_info["size_on_disk"]
    assert isinstance(size_before, int)

    utreexod.rpc.generate(EXTRA_BLOCKS)
    node_manager.wait_for_sync_nodes()

    # Poll for growth: acc roots are only written post-validation.
    wait_until(
        lambda: florestad.rpc.get_blockchain_info()["size_on_disk"] > size_before
    )
