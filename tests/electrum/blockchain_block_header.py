# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Tests for block header retrieval and validation in the Electrum server.

This tests the blockchain.block.header endpoint which returns a block header
at a given height, optionally with a merkle proof for verification.
"""

import random
import pytest

MINE_BLOCKS = 100


class TestBlockchainBlockHeader:
    """Test block header retrieval and validation."""

    log = None
    florestad = None
    utreexod = None

    @pytest.mark.electrum
    def test_blockchain_block_header(
        self, node_manager, setup_logging, florestad_utreexod
    ):
        """Test block header retrieval and validation."""
        self.log = setup_logging
        self.florestad, self.utreexod = florestad_utreexod

        self.log.info(f"Mining {MINE_BLOCKS} blocks...")
        node_manager.generate_blocks_and_sync(MINE_BLOCKS)

        self.log.info("Testing block header invalid height superior to chain height...")
        with pytest.raises(ValueError):
            self.florestad.electrum.block_header(MINE_BLOCKS + 1)

        self.log.info("Testing block header invalid height height negative...")
        with pytest.raises(ValueError):
            self.florestad.electrum.block_header(-1)

        self.log.info("Testing genesis block header...")
        self.compare_headers(0)

        self.log.info(f"Testing block header at height {MINE_BLOCKS}...")
        self.compare_headers(MINE_BLOCKS)

        random_height = random.randint(1, MINE_BLOCKS - 1)
        self.log.info(f"Testing block header at random height {random_height}...")
        self.compare_headers(random_height)

    def compare_headers(self, height):
        """Helper function to compare block headers from Electrum and RPC."""
        electrum_header = self.florestad.electrum.block_header(height)

        block_hash = self.florestad.rpc.get_blockhash(height)
        rpc_header = self.florestad.rpc.get_blockheader(block_hash, verbosity=False)

        self.log.debug(
            f"Electrum header: {electrum_header} and RPC header: {rpc_header}"
        )
        assert electrum_header == rpc_header
