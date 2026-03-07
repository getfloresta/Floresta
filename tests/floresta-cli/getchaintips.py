"""
getchaintips.py

Here we exercise the `getchaintips`, provoking it with two scenarios:
  A) Fresh node with only the genesis block
  B) After syncing a mined chain.

Both scenarios validate the full response structure (types, key set,
hash format, status values, exactly one active tip, branchlen invariants).

"""

import re
import time

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType

REGTEST_GENESIS_HASH = (
    "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
)

VALID_STATUSES = {"active", "valid-fork", "headers-only", "invalid"}


class GetChainTipsTest(FlorestaTestFramework):
    """Test the getchaintips RPC across genesis and synced chain scenarios."""

    def set_test_params(self):
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)
        self.utreexod = self.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                "--miningaddr=bcrt1q4gfcga7jfjmm02zpvrh4ttc5k7lmnq2re52z2y",
                "--utreexoproofindex",
                "--prune=0",
            ],
        )

    def validate_chain_tips_structure(self, tips):
        """Validate the structure of the getchaintips response."""
        expected_keys = {"height", "hash", "branchlen", "status"}

        self.assertTrue(isinstance(tips, list))
        self.assertTrue(len(tips) >= 1)

        active_count = 0
        for tip in tips:
            # Exactly the expected keys
            self.assertEqual(set(tip.keys()), expected_keys)

            # Type checks
            self.assertTrue(isinstance(tip["height"], int))
            self.assertTrue(isinstance(tip["branchlen"], int))
            self.assertTrue(isinstance(tip["hash"], str))
            self.assertTrue(isinstance(tip["status"], str))

            # Hash is a 64-char hex string
            self.assertTrue(bool(re.fullmatch(r"[a-f0-9]{64}", tip["hash"])))

            # Status is one of the valid values
            self.assertIn(tip["status"], VALID_STATUSES)

            # branchlen is non-negative
            self.assertTrue(tip["branchlen"] >= 0)

            if tip["status"] == "active":
                active_count += 1
                # Active tip always has branchlen 0
                self.assertEqual(tip["branchlen"], 0)

        # Exactly one active tip
        self.assertEqual(active_count, 1)

    def get_tip_by_status(self, tips, status):
        """Find the first tip with the given status."""
        for tip in tips:
            if tip["status"] == status:
                return tip
        return None

    def run_test(self):
        self.run_node(self.florestad)

        tips = self.florestad.rpc.get_chain_tips()
        self.validate_chain_tips_structure(tips)

        # Scenario A: `getchaintips()` while on genesis.
        self.assertEqual(len(tips), 1)

        active_tip = self.get_tip_by_status(tips, "active")
        self.assertIsSome(active_tip)
        self.assertEqual(active_tip["height"], 0)
        self.assertEqual(active_tip["branchlen"], 0)
        self.assertEqual(active_tip["hash"], REGTEST_GENESIS_HASH)

        # Scenario B: `getchaintips()` Synced chain with 10 blocks
        self.run_node(self.utreexod)

        self.log("Mining 10 blocks on utreexod")
        self.utreexod.rpc.generate(10)

        self.log("Connecting florestad to utreexod")
        self.connect_nodes(self.florestad, self.utreexod)

        self.log("Waiting for sync...")
        time.sleep(20)

        tips = self.florestad.rpc.get_chain_tips()
        self.validate_chain_tips_structure(tips)

        self.log("Assert: still exactly 1 tip (no forks)")
        self.assertEqual(len(tips), 1)

        active_tip = self.get_tip_by_status(tips, "active")
        self.assertIsSome(active_tip)
        self.assertEqual(active_tip["height"], 10)
        self.assertEqual(active_tip["branchlen"], 0)

        utreexo_best = self.utreexod.rpc.get_bestblockhash()
        self.assertEqual(active_tip["hash"], utreexo_best)


if __name__ == "__main__":
    GetChainTipsTest().main()
