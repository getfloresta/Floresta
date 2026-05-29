# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getblockheader.py

This functional test cli utility to interact with a Floresta node with `getblockheader`
"""

import random
import time
from typing import Any

import pytest
from requests.exceptions import HTTPError


class TestGetBlockheader:
    """Functional tests for the getblockheader RPC, comparing Florestad vs Bitcoin Core."""

    # define attributes at class level to avoid "defined outside __init__" warnings
    florestad: Any = None
    bitcoind: Any = None
    log: Any = None
    node_manager: Any = None

    @pytest.mark.rpc
    def test_get_blockheader(
        self, setup_logging, node_manager, florestad_node, bitcoind_node
    ):
        """
        Test the getblockheader RPC command. Verifies that Florestad's getblockheader RPC responses
        are compliant with Bitcoin Core's getblockheader behavior and values.
        """
        self.log = setup_logging
        self.node_manager = node_manager
        self.florestad = florestad_node
        self.bitcoind = bitcoind_node

        self.log.info("Testing getblockheader with non-existent hash")

        invalid_hash = (
            "000000000000000000015abb3038f926d74fcdc171bf6c8aadc20a9a75310ffa"
        )
        with pytest.raises(HTTPError):
            self.florestad.rpc.get_blockheader(invalid_hash)

        with pytest.raises(HTTPError):
            self.florestad.rpc.get_blockheader(invalid_hash, False)

        with pytest.raises(HTTPError):
            self.florestad.rpc.get_blockheader(invalid_hash, True)

        self.bitcoind.rpc.generate_block(2017)
        # Sleep is required to ensure blocks have different timestamps. In regtest, blocks are mined
        # almost instantaneously, so without this sleep, block timestamps would be nearly identical.
        # We need different timestamps to cause the median time of recent blocks to be different
        # from earlier blocks, which is necessary for proper testing.
        time.sleep(1)
        self.bitcoind.rpc.generate_block(5)

        self.node_manager.connect_nodes(self.florestad, self.bitcoind)

        block_count = self.bitcoind.rpc.get_block_count()
        self.node_manager.wait_for_height(self.florestad, block_count)

        self.log.info("Testing getblockheader RPC in the genesis block")
        self.validate_block_header(0)

        random_block = random.randint(1, block_count)
        self.log.info(f"Testing getblockheader RPC in block {random_block}")
        self.validate_block_header(random_block)

        self.log.info(f"Testing getblockheader RPC in block {block_count}")
        self.validate_block_header(block_count)

    def validate_block_header(self, height: int):
        """
        Compare a block header at given height between Florestad and Bitcoin Core for several
        verbosity levels.
        """
        block_hash = self.bitcoind.rpc.get_blockhash(height)
        self.log.info(
            f"Comparing block header {block_hash} between florestad and bitcoind"
        )

        self.log.info("Fetching request without verbosity")
        florestad_header = self.florestad.rpc.get_blockheader(block_hash)
        bitcoind_header = self.bitcoind.rpc.get_blockheader(block_hash)
        self.validate_headers_match(florestad_header, bitcoind_header)

        verbosity = False
        self.log.info(f"Fetching request with verbosity {verbosity}")
        florestad_header = self.florestad.rpc.get_blockheader(block_hash, verbosity)
        bitcoind_header = self.bitcoind.rpc.get_blockheader(block_hash, verbosity)
        assert florestad_header == bitcoind_header

        verbosity = True
        self.log.info(f"Fetching request with verbosity {verbosity}")
        florestad_header = self.florestad.rpc.get_blockheader(block_hash, verbosity)
        bitcoind_header = self.bitcoind.rpc.get_blockheader(block_hash, verbosity)

        self.validate_headers_match(florestad_header, bitcoind_header)

    def validate_headers_match(self, florestad_header: dict, bitcoind_header: dict):
        """
        Compare two block headers and assert that they match.
        """
        for key, bval in bitcoind_header.items():
            fval = florestad_header[key]

            self.log.info(f"Comparing {key} field: florestad={fval} bitcoind={bval}")
            if key == "difficulty":
                # Allow small differences in floating point representation
                assert round(fval, 3) == round(bval, 3)
            else:
                assert fval == bval
