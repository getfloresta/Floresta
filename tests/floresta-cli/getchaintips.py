"""
getchaintips.py

Tests `getchaintips` RPC by running identical operations on both florestad
and bitcoind, comparing their outputs to ensure floresta matches Bitcoin
Core's behavior.

Scenarios:
  A) Only genesis block exists
  B) Synced a 10-block chain (no forks)
  C) Submit a header for a block the node doesn't have yet
  D) Use invalidateblock to produce an "invalid" chain tip
  E) Create one fork by invalidating block 5 and mining 10 new blocks
  F) Create a second fork by invalidating block 8 and mining 10 more
"""

import re
import time

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType

REGTEST_GENESIS_HASH = (
    "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
)

VALID_STATUSES = {"active", "valid-fork", "headers-only", "invalid", "valid-headers"}


class GetChainTipsTest(FlorestaTestFramework):
    """Test the getchaintips RPC by comparing florestad with bitcoind."""

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
        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def tips_by_status(self, tips, status):
        """Find all tips with the given status."""
        return [t for t in tips if t["status"] == status]

    def tip_by_hash(self, tips, block_hash):
        """Find a tip by its block hash."""
        for t in tips:
            if t["hash"] == block_hash:
                return t
        return None

    def log_tips(self, label, tips):
        """Pretty-print chain tips."""
        self.log(f"{label}:")
        for t in tips:
            self.log(
                f"  height={t['height']} status={t['status']} "
                f"branchlen={t['branchlen']} hash={t['hash'][:16]}..."
            )

    def run_test(self):
        # Start all nodes
        self.run_node(self.florestad)
        self.run_node(self.utreexod)
        self.run_node(self.bitcoind)

        # ── Scenario A: genesis only ─────────────────────────────────
        self.log("=== Scenario A: genesis only")

        f_tips = self.florestad.rpc.get_chain_tips()
        b_tips = self.bitcoind.rpc.get_chain_tips()

        self.log_tips("florestad", f_tips)
        self.log_tips("bitcoind", b_tips)

        self.assertEqual(len(f_tips), 1)
        self.assertEqual(len(b_tips), 1)
        self.assertEqual(f_tips[0]["status"], b_tips[0]["status"])
        self.assertEqual(f_tips[0]["height"], b_tips[0]["height"])

        # ── Scenario B: synced 10-block chain ────────────────────────
        self.log("=== Scenario B: synced chain, no forks")

        self.log("Mining 10 blocks on utreexod")
        self.utreexod.rpc.generate(10)

        # Connect both florestad and bitcoind to utreexod
        self.log("Connecting florestad to utreexod")
        self.connect_nodes(self.florestad, self.utreexod)

        self.log("Connecting bitcoind to utreexod")
        self.connect_nodes(self.bitcoind, self.utreexod)

        self.log("Waiting for sync...")
        time.sleep(20)

        f_tips = self.florestad.rpc.get_chain_tips()
        b_tips = self.bitcoind.rpc.get_chain_tips()

        self.log_tips("florestad", f_tips)
        self.log_tips("bitcoind", b_tips)

        self.assertEqual(len(f_tips), 1)
        self.assertEqual(len(b_tips), 1)
        self.assertEqual(f_tips[0]["status"], b_tips[0]["status"])
        self.assertEqual(f_tips[0]["height"], b_tips[0]["height"])

        utreexo_best = self.utreexod.rpc.get_bestblockhash()

        # ── Scenario C: submit_header ────────────────────────────────
        self.log("=== Scenario C: submit_header for unknown block")

        # Mine 1 more block on utreexod but DON'T let florestad/bitcoind
        # sync it via P2P yet — we only submit the header.
        new_hashes = self.utreexod.rpc.generate(1)
        block_11_hash = new_hashes[0]

        # Get the raw header (first 160 hex chars of raw block)
        raw_block = self.utreexod.rpc.get_block(block_11_hash, 0)
        header_hex = raw_block[:160]

        # Submit header to both
        self.florestad.rpc.submit_header(header_hex)
        self.bitcoind.rpc.submit_header(header_hex)

        f_tips = self.florestad.rpc.get_chain_tips()
        b_tips = self.bitcoind.rpc.get_chain_tips()

        self.log_tips("florestad", f_tips)
        self.log_tips("bitcoind", b_tips)

        # Log the differences for analysis — don't assert equality yet,
        # we want to SEE what each node reports.
        self.log(
            f"florestad tip count: {len(f_tips)}, bitcoind tip count: {len(b_tips)}"
        )

        # Now let both nodes sync the full block
        self.log("Waiting for sync of block 11...")
        time.sleep(20)

        f_tips = self.florestad.rpc.get_chain_tips()
        b_tips = self.bitcoind.rpc.get_chain_tips()

        self.log_tips("florestad (after sync)", f_tips)
        self.log_tips("bitcoind (after sync)", b_tips)

        # ── Scenario D: invalidateblock ──────────────────────────────
        self.log("=== Scenario D: invalid status via invalidateblock")

        block_at_8 = self.florestad.rpc.get_blockhash(8)
        self.florestad.rpc.invalidate_block(block_at_8)
        self.bitcoind.rpc.invalidate_block(block_at_8)

        f_tips = self.florestad.rpc.get_chain_tips()
        b_tips = self.bitcoind.rpc.get_chain_tips()

        self.log_tips("florestad", f_tips)
        self.log_tips("bitcoind", b_tips)

        self.log(
            f"florestad tip count: {len(f_tips)}, bitcoind tip count: {len(b_tips)}"
        )

        # ── Scenario E: fork via invalidation at height 5 ───────────
        self.log("=== Scenario E: single fork via invalidation at height 5")

        block_at_5 = self.utreexod.rpc.get_blockhash(5)
        self.utreexod.rpc.invalidate_block(block_at_5)

        self.log("Mining 10 blocks on new chain")
        self.utreexod.rpc.generate(10)

        self.log("Waiting for sync...")
        time.sleep(20)

        f_tips = self.florestad.rpc.get_chain_tips()
        b_tips = self.bitcoind.rpc.get_chain_tips()

        self.log_tips("florestad", f_tips)
        self.log_tips("bitcoind", b_tips)

        self.log(
            f"florestad tip count: {len(f_tips)}, bitcoind tip count: {len(b_tips)}"
        )

        # ── Scenario F: second fork via invalidation at height 8 ─────
        self.log("=== Scenario F: second fork via invalidation at height 8")

        block_at_8 = self.utreexod.rpc.get_blockhash(8)
        self.utreexod.rpc.invalidate_block(block_at_8)

        self.log("Mining 10 blocks on second alternative chain")
        self.utreexod.rpc.generate(10)

        self.log("Waiting for sync...")
        time.sleep(20)

        f_tips = self.florestad.rpc.get_chain_tips()
        b_tips = self.bitcoind.rpc.get_chain_tips()

        self.log_tips("florestad", f_tips)
        self.log_tips("bitcoind", b_tips)

        self.log(
            f"florestad tip count: {len(f_tips)}, bitcoind tip count: {len(b_tips)}"
        )


if __name__ == "__main__":
    GetChainTipsTest().main()
