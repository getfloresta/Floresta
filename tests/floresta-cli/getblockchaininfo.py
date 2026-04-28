# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getblockchaininfo.py

This functional test cli utility to interact with a Floresta node with `getblockchaininfo`
"""

import time
from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class GetBlockchaininfoTest(FlorestaTestFramework):
    """
    Test `getblockchaininfo` in two phases:
    1. Genesis state  verify all fields against known regtest values
    2. After sync  mine blocks, sync Floresta with utreexod/bitcoind, then compare shared fields across all three nodes
    """

    nodes = [-1]
    bestblockhash = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
    difficulty = 4.6565423739069247e-10
    blocks = 0
    initialblockdownload = True
    time = 1296688602
    chainwork = "0000000000000000000000000000000000000000000000000000000000000002"
    verificationprogress = 0
    headers = 0
    mediantime = 1296688602
    bits = "207fffff"
    target = "7fffff0000000000000000000000000000000000000000000000000000000000"
    pruned = True
    pruneheight = 1
    automatic_pruning = True
    warnings = []

    def assert_fields(self, response, expected):
        """
        Assert all fields in `expected` against `response`, with an approximate
        comparison for `difficulty` to handle float precision differences between
        Rust and Python/Core.
        """
        for key, value in expected.items():
            if key == "difficulty":
                self.assertTrue(abs(response[key] - value) < 1e-20)
            else:
                self.assertEqual(response[key], value)

    def set_test_params(self):
        """
        Setup a single node
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

        self.utreexod = self.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                "--miningaddr=bcrt1q4gfcga7jfjmm02zpvrh4ttc5k7lmnq2re52z2y",
                "--prune=0",
            ],
        )

        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def run_test(self):
        """
        Phase 1: Test genesis state fields
        Phase 2: Mine blocks, sync, and compare fields with bitcoind
        """

        # Genesis State Test (Phase 1)
        self.run_node(self.florestad)

        response = self.florestad.rpc.get_blockchain_info()
        self.assert_fields(
            response,
            {
                "bestblockhash": GetBlockchaininfoTest.bestblockhash,
                "difficulty": GetBlockchaininfoTest.difficulty,
                "blocks": GetBlockchaininfoTest.blocks,
                "initialblockdownload": GetBlockchaininfoTest.initialblockdownload,
                "time": GetBlockchaininfoTest.time,
                "chainwork": GetBlockchaininfoTest.chainwork,
                "verificationprogress": GetBlockchaininfoTest.verificationprogress,
                "headers": GetBlockchaininfoTest.headers,
                "mediantime": GetBlockchaininfoTest.mediantime,
                "bits": GetBlockchaininfoTest.bits,
                "target": GetBlockchaininfoTest.target,
                "pruned": GetBlockchaininfoTest.pruned,
                "warnings": GetBlockchaininfoTest.warnings,
                "pruneheight": GetBlockchaininfoTest.pruneheight,
                "automatic_pruning": GetBlockchaininfoTest.automatic_pruning,
                "chain": "regtest",
            },
        )

        self.assertTrue(response["size_on_disk"] > 0)

        self.log("=== Starting utreexod and bitcoind")
        self.run_node(self.utreexod)
        self.run_node(self.bitcoind)

        self.log("=== Mining 10 blocks with utreexod")
        self.utreexod.rpc.generate(10)
        time.sleep(5)

        self.log("=== Connecting nodes")
        self.connect_nodes(self.florestad, self.utreexod)
        self.connect_nodes(self.bitcoind, self.utreexod)

        self.log("=== Waiting for sync")
        time.sleep(20)

        self.log("=== Comparing shared fields across all three nodes")
        floresta = self.florestad.rpc.get_blockchain_info()
        bitcoind = self.bitcoind.rpc.get_blockchain_info()
        utreexod = self.utreexod.rpc.get_blockchain_info()

        self.assertEqual(floresta["bestblockhash"], utreexod["bestblockhash"])
        self.assert_fields(
            floresta,
            {
                field: bitcoind[field]
                for field in [
                    "bestblockhash",
                    "blocks",
                    "chain",
                    "chainwork",
                    "mediantime",
                    "time",
                    "bits",
                    "target",
                    "headers",
                    "difficulty",
                    "initialblockdownload",
                    "verificationprogress",
                ]
            },
        )
        self.assertEqual(floresta["blocks"], 10)

        self.log("=== Check pruning fields")
        self.assertEqual(floresta["pruned"], True)
        self.assertEqual(floresta["automatic_pruning"], True)
        self.assertEqual(floresta["prune_target_size"], 0)
        self.assertEqual(floresta["pruneheight"], 11)

        self.assertTrue(floresta["size_on_disk"] > 0)


if __name__ == "__main__":
    GetBlockchaininfoTest().main()
