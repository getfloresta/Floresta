# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getindexinfo.py

This functional test validates the `getindexinfo` RPC call, both on a fresh
node and after syncing blocks from a utreexod peer.
"""

import time

import pytest

MINE_BLOCKS = 10
TIMEOUT_SECONDS = 20


def validate_index_entry(entry):
    """Validate that an index entry has the correct structure."""
    assert set(entry.keys()) == {"synced", "best_block_height"}
    assert isinstance(entry["synced"], bool)
    assert isinstance(entry["best_block_height"], int)
    assert entry["best_block_height"] >= 0


@pytest.mark.rpc
def test_get_index_info_structure(florestad_node):
    """
    Test `getindexinfo` returns available indices with the expected structure
    on a fresh node.
    """
    result = florestad_node.rpc.get_indexinfo()

    assert isinstance(result, dict)

    # Each present index should have the correct structure
    for entry in result.values():
        validate_index_entry(entry)


@pytest.mark.rpc
def test_get_index_info_unknown_name(florestad_node):
    """
    Test that requesting an unknown index name returns an empty object.
    """
    result = florestad_node.rpc.get_indexinfo("nonexistent_index")

    assert isinstance(result, dict)
    assert result == {}


@pytest.mark.rpc
def test_get_index_info_after_sync(florestad_utreexod):
    """
    Test that index heights advance after mining blocks on utreexod and
    syncing them to florestad.
    """
    florestad, utreexod = florestad_utreexod

    # Mine blocks on utreexod
    utreexod.rpc.generate(MINE_BLOCKS)

    # Wait for florestad to sync the chain
    timeout = time.time() + TIMEOUT_SECONDS
    while time.time() < timeout:
        if florestad.rpc.get_block_count() == MINE_BLOCKS:
            break
        time.sleep(1)

    assert florestad.rpc.get_block_count() == MINE_BLOCKS

    result = florestad.rpc.get_indexinfo()

    # If block_filter is present, it should have caught up
    if "block_filter" in result:
        block_filter = result["block_filter"]
        assert block_filter["best_block_height"] == MINE_BLOCKS
        assert block_filter["synced"] is True


@pytest.mark.rpc
def test_get_index_info_filter_after_sync(florestad_utreexod):
    """
    Test that filtering by name works correctly after syncing,
    and unknown names still return empty.
    """
    florestad, utreexod = florestad_utreexod

    utreexod.rpc.generate(MINE_BLOCKS)

    timeout = time.time() + TIMEOUT_SECONDS
    while time.time() < timeout:
        if florestad.rpc.get_block_count() == MINE_BLOCKS:
            break
        time.sleep(1)

    assert florestad.rpc.get_block_count() == MINE_BLOCKS

    # If block_filter is available, filter for it specifically
    all_indices = florestad.rpc.get_indexinfo()
    if "block_filter" in all_indices:
        result = florestad.rpc.get_indexinfo("block_filter")
        assert len(result) == 1
        assert result["block_filter"]["best_block_height"] == MINE_BLOCKS
        assert result["block_filter"]["synced"] is True

    # Unknown name should still return empty after sync
    result = florestad.rpc.get_indexinfo("txindex")
    assert result == {}
