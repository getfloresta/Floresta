"""
submitheader.py

Test the `submitheader` RPC on florestad.

We mine a block on bitcoind to obtain a valid header, then shut it down.
We start florestad in isolation and exercise submitheader:
  - reject invalid hex
  - reject truncated (non-80-byte) data
  - reject a header whose prev_blockhash is unknown
  - accept a valid header and advance the tip to height 1
  - accept a duplicate submission idempotently
"""

from requests import HTTPError
from test_framework import FlorestaTestFramework
from test_framework.bitcoin import BlockHeader
from test_framework.node import NodeType

REGTEST_GENESIS = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"


class SubmitHeaderTest(FlorestaTestFramework):
    """Test that submitheader accepts valid headers and rejects invalid ones."""

    def set_test_params(self):
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)
        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def run_test(self):
        # Start bitcoind, mine 1 block, grab its header
        self.run_node(self.bitcoind)

        self.log("=== Mining 1 block with bitcoind")
        self.bitcoind.rpc.generate_block(1)

        block_hash = self.bitcoind.rpc.get_blockhash(1)
        raw_block_hex = self.bitcoind.rpc.get_block(block_hash, 0)

        # First 160 hex chars = 80-byte block header
        header_hex = raw_block_hex[:160]

        self.log("=== Stopping bitcoind")
        self.bitcoind.stop()

        # Start florestad in isolation
        self.run_node(self.florestad)

        info_before = self.florestad.rpc.get_blockchain_info()
        self.assertEqual(info_before["height"], 0)

        # --- Error case 1: invalid hex ---
        self.log("=== Submitting invalid hex string")
        with self.assertRaises(HTTPError):
            self.florestad.rpc.submit_header("zzzz_not_hex")

        # --- Error case 2: valid hex but wrong length (not 80 bytes) ---
        self.log("=== Submitting truncated header (too short)")
        with self.assertRaises(HTTPError):
            self.florestad.rpc.submit_header(header_hex[:80])

        # --- Error case 3: 80-byte header with unknown prev_blockhash ---
        self.log("=== Submitting header with unknown prev_blockhash")
        header = BlockHeader.deserialize(bytes.fromhex(header_hex))
        header.prev_blockhash = "aa" * 32  # unknown parent
        bad_parent_hex = header.serialize().hex()
        with self.assertRaises(HTTPError):
            self.florestad.rpc.submit_header(bad_parent_hex)

        # --- Happy path: valid header ---
        self.log(f"=== Submitting valid header for block {block_hash}")
        self.florestad.rpc.submit_header(header_hex)

        info_after = self.florestad.rpc.get_blockchain_info()
        self.assertEqual(info_after["height"], 1)
        self.assertEqual(info_after["best_block"], block_hash)

        # --- Submitting the same header again, should succeed. ---
        self.log("=== Submitting the same header again (duplicate)")
        self.florestad.rpc.submit_header(header_hex)

        info_dup = self.florestad.rpc.get_blockchain_info()
        self.assertEqual(info_dup["height"], 1)
        self.assertEqual(info_dup["best_block"], block_hash)


if __name__ == "__main__":
    SubmitHeaderTest().main()
