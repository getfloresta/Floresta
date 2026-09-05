# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getchaintips.py

Functional tests for `getchaintips`. Verifies the RPC returns correct chain tip
information for both a single-tip chain and after a chain reorganization that
produces multiple tips.

The genesis and single-tip tests share a single set of class-scoped nodes.
The reorg test uses its own function-scoped two-node network so that reorg
synchronisation is reliable (matching the proven pattern in reorg_chain.py).
"""

import pytest
from test_framework.constants import GENESIS_BLOCK_HASH

# pylint: disable=redefined-outer-name

MINE_BLOCKS = 10


def assert_active_tip(tip, expected_height, expected_hash):
    """Assert a tip entry has active status with the expected height and hash."""
    assert tip["status"] == "active"
    assert tip["branchlen"] == 0
    assert tip["height"] == expected_height
    assert tip["hash"] == expected_hash


@pytest.fixture(scope="class")
def setup_nodes(
    shared_setup_logging,
    shared_florestad_bitcoind_utreexod_with_chain,
):
    """Set up logging and the three-node network synced to MINE_BLOCKS blocks."""
    return shared_setup_logging, shared_florestad_bitcoind_utreexod_with_chain(
        MINE_BLOCKS
    )


@pytest.mark.rpc
class TestGetChainTips:
    """Tests for the getchaintips RPC command."""

    def test_at_genesis(self, shared_florestad_node):
        """
        At genesis (no blocks mined), getchaintips should return a single
        active tip at height 0 with the genesis block hash.
        """
        tips = shared_florestad_node.rpc.get_chain_tips()

        assert isinstance(tips, list)
        assert len(tips) == 1
        assert_active_tip(tips[0], 0, GENESIS_BLOCK_HASH)

    def test_single_tip(self, setup_nodes):
        """
        After mining blocks on a single chain (no forks), getchaintips should
        return exactly one tip with status "active" and branchlen 0.
        """
        _log, (florestad, _bitcoind, _utreexod) = setup_nodes

        tips = florestad.rpc.get_chain_tips()

        assert isinstance(tips, list)
        assert len(tips) == 1
        assert_active_tip(
            tips[0],
            florestad.rpc.get_block_count(),
            florestad.rpc.get_bestblockhash(),
        )


@pytest.mark.rpc
def test_getchaintips_after_reorg(setup_logging, florestad_utreexod, node_manager):
    """
    Trigger a chain reorganization and verify that getchaintips reports
    multiple tips: the active tip and at least one fork tip with
    status "valid-headers" and branchlen > 0.
    """
    log = setup_logging
    florestad, utreexod = florestad_utreexod

    # Mine initial blocks and wait for sync
    utreexod.rpc.generate(MINE_BLOCKS)
    node_manager.wait_for_sync_nodes(is_finished_ibd=False)

    old_best = florestad.rpc.get_bestblockhash()

    # Invalidate a block to create a fork point
    count_invalid = 5
    height_invalid = utreexod.rpc.get_block_count() - count_invalid
    log.info(f"Invalidating block at height {height_invalid}")
    utreexod.rpc.invalidate_block(utreexod.rpc.get_blockhash(height_invalid))

    # Mine a longer alternative chain to trigger reorg
    new_blocks = count_invalid + 5
    log.info(f"Mining {new_blocks} blocks on the alternative chain")
    utreexod.rpc.generate(new_blocks)
    node_manager.wait_for_sync_nodes(is_finished_ibd=False)

    # The best block should have changed
    assert florestad.rpc.get_bestblockhash() != old_best

    tips = florestad.rpc.get_chain_tips()
    assert isinstance(tips, list)
    assert len(tips) >= 2

    # Exactly one tip should be active
    active_tips = [t for t in tips if t["status"] == "active"]
    assert len(active_tips) == 1
    assert_active_tip(
        active_tips[0],
        active_tips[0]["height"],
        florestad.rpc.get_bestblockhash(),
    )
    assert active_tips[0]["hash"] == utreexod.rpc.get_bestblockhash()

    # At least one fork tip should exist with valid-headers status
    fork_tips = [t for t in tips if t["status"] == "valid-headers"]
    assert len(fork_tips) >= 1

    for fork_tip in fork_tips:
        assert fork_tip["branchlen"] > 0
        assert fork_tip["height"] > 0
