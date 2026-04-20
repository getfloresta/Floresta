# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getdifficulty.py

This functional test exercises the `getdifficulty` RPC by starting a Floresta
node and a bitcoind node, then verifying both report the same difficulty
value for parity with Bitcoin Core.
"""

import math

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class GetDifficultyTest(FlorestaTestFramework):
    """
    Test `getdifficulty` by comparing florestad's response against bitcoind's
    on the same network. Both nodes start at the regtest genesis block, so
    they should report the same difficulty for that target.
    """

    # Bitcoin Core and rust-bitcoin compute difficulty with slightly different
    # float arithmetic. Allow a small relative tolerance on the parity check.
    tolerance = 1e-9

    def set_test_params(self):
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)
        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def run_test(self):
        # Start florestad and bitcoind; both should start on the regtest
        # genesis block and report a positive difficulty value.
        self.run_node(self.florestad)
        self.run_node(self.bitcoind)

        floresta_difficulty = self.florestad.rpc.get_difficulty()
        bitcoind_difficulty = self.bitcoind.rpc.get_difficulty()
        self.log(
            f"[genesis] florestad={floresta_difficulty}, bitcoind={bitcoind_difficulty}"
        )

        self.assertIsSome(floresta_difficulty)
        self.assertTrue(floresta_difficulty > 0)
        self.assertTrue(
            math.isclose(
                floresta_difficulty, bitcoind_difficulty, rel_tol=self.tolerance
            )
        )

        # Mine past the 2016-block retarget boundary and confirm difficulty
        baseline = bitcoind_difficulty
        for target_height in (100, 1000, 2016, 3000):
            current = self.bitcoind.rpc.get_block_count()
            to_mine = target_height - current
            if to_mine > 0:
                self.bitcoind.rpc.generate_block(to_mine)
            height = self.bitcoind.rpc.get_block_count()
            difficulty = self.bitcoind.rpc.get_difficulty()
            self.log(f"[height={height}] bitcoind difficulty={difficulty}")
            self.assertTrue(math.isclose(difficulty, baseline, rel_tol=self.tolerance))


if __name__ == "__main__":
    GetDifficultyTest().main()
