"""
floresta_cli_getblock.py

This functional test cli utility to interact with a Floresta node with `getblock`
"""

import time
import random
from typing import Any

import pytest


class TestGetBlock:
    """Functional tests for the getblock RPC, comparing Florestad vs Bitcoin Core."""

    # define attributes at class level to avoid "defined outside __init__" warnings
    florestad: Any = None
    bitcoind: Any = None
    log: Any = None
    node_manager: Any = None

    @pytest.mark.rpc
    def test_get_block(
        self, florestad_node, bitcoind_node, setup_logging, node_manager
    ):
        """
        Test the getblock RPC command. Verifies that Florestad's getblock RPC responses are
        compliant with Bitcoin Core's getblock behavior and values.
        """
        self.florestad = florestad_node
        self.bitcoind = bitcoind_node
        self.log = setup_logging
        self.node_manager = node_manager

        self.bitcoind.rpc.generate_block(2017)
        time.sleep(1)
        self.bitcoind.rpc.generate_block(6)

        self.node_manager.connect_nodes(self.florestad, self.bitcoind)

        block_count = self.bitcoind.rpc.get_block_count()
        end = time.time() + 20
        while time.time() < end:
            floresta_count = self.florestad.rpc.get_block_count()
            if floresta_count == block_count:
                break
            time.sleep(0.5)

        assert floresta_count == block_count

        self.log.info("Testing getblock RPC in the genesis block")
        self.compare_block(0)

        random_block = random.randint(1, block_count)
        self.log.info(f"Testing getblock RPC in block {random_block}")
        self.compare_block(random_block)

        self.log.info(f"Testing getblock RPC in block {block_count}")
        self.compare_block(block_count)

    def compare_block(self, height: int):
        """
        Compare a block at given height between Florestad and Bitcoin Core for several
        verbosity levels.
        """
        block_hash = self.bitcoind.rpc.get_blockhash(height)
        self.log.info(f"Comparing block {block_hash} between florestad and bitcoind")

        self.log.info("Fetching request with verbosity 0")
        floresta_block = self.florestad.rpc.get_block(block_hash, 0)
        bitcoind_block = self.bitcoind.rpc.get_block(block_hash, 0)
        assert floresta_block == bitcoind_block

        self.log.info("Fetching request with verbosity 1")
        floresta_block = self.florestad.rpc.get_block(block_hash, 1)
        bitcoind_block = self.bitcoind.rpc.get_block(block_hash, 1)

        for key, bval in bitcoind_block.items():
            fval = floresta_block[key]

            self.log.info(f"Comparing {key} field: floresta={fval} bitcoind={bval}")
            if key == "difficulty":
                # Allow small differences in floating point representation
                assert round(fval, 3) == round(bval, 3)
            else:
                assert fval == bval
