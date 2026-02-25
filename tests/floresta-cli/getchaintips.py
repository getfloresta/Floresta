"""
getchaintips.py

Tests `getchaintips` RPC through four scenarios:
  A) Only genesis block exists
  B) Synced a 10-block chain (no forks)
  C) Create one fork by invalidating block 5 and mining 10 new blocks
  D) Create a second fork by invalidating block 8 and mining 10 more

Each scenario checks the response shape and that fork tips are correct.
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
            else:
                # Non-active tips must have branchlen > 0
                self.assertTrue(tip["branchlen"] > 0)

        # Exactly one active tip
        self.assertEqual(active_count, 1)

    def get_tip_by_status(self, tips, status):
        """Find the first tip with the given status."""
        for tip in tips:
            if tip["status"] == status:
                return tip
        return None

    def get_tip_by_hash(self, tips, block_hash):
        """Find a tip by its block hash."""
        for tip in tips:
            if tip["hash"] == block_hash:
                return tip
        return None

    def get_tips_by_status(self, tips, status):
        """Find all tips with the given status."""
        return [tip for tip in tips if tip["status"] == status]

    def run_test(self):
        self.run_node(self.florestad)

        tips = self.florestad.rpc.get_chain_tips()
        self.validate_chain_tips_structure(tips)

        # Scenario A: `getchaintips()` while on genesis.
        self.log("=== Scenario A: genesis only")
        self.assertEqual(len(tips), 1)

        active_tip = self.get_tip_by_status(tips, "active")
        self.assertIsSome(active_tip)
        self.assertEqual(active_tip["height"], 0)
        self.assertEqual(active_tip["branchlen"], 0)
        self.assertEqual(active_tip["hash"], REGTEST_GENESIS_HASH)

        # Scenario B: `getchaintips()` on synced chain with 10 blocks
        self.log("=== Scenario B: synced chain, no forks")
        self.run_node(self.utreexod)

        self.log("Mining 10 blocks on utreexod")
        self.utreexod.rpc.generate(10)

        self.log("Connecting florestad to utreexod")
        self.connect_nodes(self.florestad, self.utreexod)

        self.log("Waiting for sync...")
        time.sleep(20)

        tips = self.florestad.rpc.get_chain_tips()
        self.validate_chain_tips_structure(tips)

        self.log("Assert: exactly 1 tip (no forks)")
        self.assertEqual(len(tips), 1)

        active_tip = self.get_tip_by_status(tips, "active")
        self.assertIsSome(active_tip)
        self.assertEqual(active_tip["height"], 10)
        self.assertEqual(active_tip["branchlen"], 0)

        utreexo_best = self.utreexod.rpc.get_bestblockhash()
        self.assertEqual(active_tip["hash"], utreexo_best)

        # Scenario C: Invalidate block 5 and mine 10 new blocks.
        # The chain splits at height 4: the new branch grows to height 14
        # and becomes active, the old blocks 5..10 become a fork tip.
        #
        self.log("=== Scenario C: single fork via invalidation at height 5")

        block_at_5 = self.utreexod.rpc.get_blockhash(5)
        self.utreexod.rpc.invalidate_block(block_at_5)

        self.log("Mining 10 blocks on new chain")
        new_hashes = self.utreexod.rpc.generate(10)

        self.log("Waiting for sync...")
        time.sleep(20)

        tips = self.florestad.rpc.get_chain_tips()
        self.log(f"Chain tips after first fork: {tips}")
        self.validate_chain_tips_structure(tips)

        self.log("Assert: exactly 2 tips (1 fork)")
        self.assertEqual(len(tips), 2)

        # The new chain tip at height 14
        utreexo_best = self.utreexod.rpc.get_bestblockhash()
        active_tip = self.get_tip_by_hash(tips, utreexo_best)
        self.assertIsSome(active_tip)
        self.assertEqual(active_tip["status"], "active")
        self.assertEqual(active_tip["height"], 14)
        self.assertEqual(active_tip["branchlen"], 0)

        # The old tip at height 10 is now a fork tip
        fork_hash = new_hashes[5]  # height 10 on the new chain
        fork_tip = self.get_tip_by_hash(tips, fork_hash)
        self.assertIsSome(fork_tip)
        self.assertEqual(fork_tip["status"], "valid-fork")
        self.assertEqual(fork_tip["height"], 10)
        self.assertEqual(fork_tip["branchlen"], 1)

        # Scenario D: Invalidate block 8 and mine 10 more.
        # The chain now splits at height 7: the new branch grows to 17
        # (active), old blocks 8..14 become a second fork tip, and the
        # fork from scenario C is still around at height 10.
        self.log("=== Scenario D: second fork via invalidation at height 8")

        block_at_8 = self.utreexod.rpc.get_blockhash(8)
        self.utreexod.rpc.invalidate_block(block_at_8)

        self.log("Mining 10 blocks on second alternative chain")
        new_hashes_d = self.utreexod.rpc.generate(10)

        self.log("Waiting for sync...")
        time.sleep(20)

        tips = self.florestad.rpc.get_chain_tips()
        self.log(f"Chain tips after second fork: {tips}")
        self.validate_chain_tips_structure(tips)

        self.log("Assert: exactly 3 tips (2 forks)")
        self.assertEqual(len(tips), 3)

        # The new chain tip at height 17
        utreexo_best = self.utreexod.rpc.get_bestblockhash()
        active_tip = self.get_tip_by_hash(tips, utreexo_best)
        self.assertIsSome(active_tip)
        self.assertEqual(active_tip["status"], "active")
        self.assertEqual(active_tip["height"], 17)
        self.assertEqual(active_tip["branchlen"], 0)

        # The old tip at height 14 is now a fork tip
        fork_hash_d = new_hashes_d[6]  # height 14 on the newest chain
        fork_tip_d = self.get_tip_by_hash(tips, fork_hash_d)
        self.assertIsSome(fork_tip_d)
        self.assertEqual(fork_tip_d["status"], "valid-fork")
        self.assertEqual(fork_tip_d["height"], 14)
        self.assertEqual(fork_tip_d["branchlen"], 1)

        # The fork from scenario C is still here, but its branchlen
        # grew from 1 to 3 because the active chain now diverges at
        # height 7 instead of height 4.
        fork_tip_c = self.get_tip_by_hash(tips, fork_hash)
        self.assertIsSome(fork_tip_c)
        self.assertEqual(fork_tip_c["status"], "valid-fork")
        self.assertEqual(fork_tip_c["height"], 10)
        self.assertEqual(fork_tip_c["branchlen"], 3)


if __name__ == "__main__":
    GetChainTipsTest().main()
