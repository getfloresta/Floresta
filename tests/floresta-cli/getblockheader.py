# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getblockheader.py

This functional test cli utility to interact with a Floresta node with `getblockheader`
"""

import time
import random

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class GetBlockheaderTest(FlorestaTestFramework):

    def set_test_params(self):

        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)
        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def validate_block_header(self, height: int):
        block_hash = self.bitcoind.rpc.get_blockhash(height)
        self.log(f"Comparing block header {block_hash} between florestad and bitcoind")

        self.log("Fetching request without verbosity")
        florestad_header = self.florestad.rpc.get_blockheader(block_hash)
        bitcoind_header = self.bitcoind.rpc.get_blockheader(block_hash)
        self.validate_headers_match(florestad_header, bitcoind_header)

        self.log("Fetching request with verbosity false")
        florestad_header = self.florestad.rpc.get_blockheader(block_hash, False)
        bitcoind_header = self.bitcoind.rpc.get_blockheader(block_hash, False)
        self.assertEqual(florestad_header, bitcoind_header)

        self.log("Fetching request with verbosity true")
        florestad_header = self.florestad.rpc.get_blockheader(block_hash, True)
        bitcoind_header = self.bitcoind.rpc.get_blockheader(block_hash, True)

        self.validate_headers_match(florestad_header, bitcoind_header)

    def validate_headers_match(self, florestad_header: dict, bitcoind_header: dict):
        for key, bval in bitcoind_header.items():
            fval = florestad_header[key]

            self.log(f"Comparing {key} field: florestad={fval} bitcoind={bval}")
            if key == "difficulty":
                # Allow small differences in floating point representation
                self.assertEqual(round(fval, 3), round(bval, 3))
            else:
                self.assertEqual(fval, bval)

    def run_test(self):
        self.run_node(self.florestad)
        self.run_node(self.bitcoind)

        self.bitcoind.rpc.generate_block(2017)
        time.sleep(1)
        self.bitcoind.rpc.generate_block(6)

        self.log("Connecting florestad to bitcoind")
        self.connect_nodes(self.florestad, self.bitcoind)

        block_count = self.bitcoind.rpc.get_block_count()
        end = time.time() + 20
        while time.time() < end:
            if self.florestad.rpc.get_block_count() == block_count:
                break
            time.sleep(0.5)

        self.assertEqual(
            self.florestad.rpc.get_block_count(), self.bitcoind.rpc.get_block_count()
        )

        self.log("Testing getblockheader RPC in the genesis block")
        self.validate_block_header(0)

        random_block = random.randint(1, block_count)
        self.log(f"Testing getblockheader RPC in block {random_block}")
        self.validate_block_header(random_block)

        self.log(f"Testing getblockheader RPC in block {block_count}")
        self.validate_block_header(block_count)


if __name__ == "__main__":
    GetBlockheaderTest().main()
