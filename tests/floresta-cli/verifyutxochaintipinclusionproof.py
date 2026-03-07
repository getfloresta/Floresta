"""
Test verifyutxochaintipinclusionproof RPC.
"""

import time
from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from requests.exceptions import HTTPError

MINING_ADDR = "bcrt1p5q9rq6tja29u2dafp0j7a2ul6qtuuna62m7ky8f6yqrvwf5j7knqmv2lus"


class VerifyUtxoChainTipInclusionProofTest(FlorestaTestFramework):
    """
    Test verifyutxochaintipinclusionproof RPC with valid and invalid inputs.
    """

    NUM_BLOCKS = 10

    def set_test_params(self):
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)
        self.utreexod = self.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                "--flatutreexoproofindex",
                "--cfilters",
                "--prune=0",
                f"--miningaddr={MINING_ADDR}",
            ],
        )

    def wait_for_sync_and_check(self):
        """Poll until Floresta has validated up to the given height."""
        height = self.utreexod.rpc.get_block_count()
        end = time.time() + 120
        info = None
        while time.time() < end:
            try:
                info = self.florestad.rpc.get_blockchain_info()
                if info["validated"] == height and not info["ibd"]:
                    break
            except Exception:
                pass
            time.sleep(3)
        self.assertTrue(info is not None)
        self.assertEqual(info["validated"], height)
        self.assertFalse(info["ibd"])

    def get_coinbase_txids(self):
        """Retrieve coinbase transaction IDs for NUM_BLOCKS."""
        txids = []
        for height in range(1, self.NUM_BLOCKS + 1):
            block_hash = self.florestad.rpc.get_blockhash(height)
            block = self.florestad.rpc.get_block(block_hash, 1)
            txids.append(block["tx"][0])
        return txids

    def run_test(self):
        self.run_node(self.florestad)
        self.run_node(self.utreexod)

        self.log("Input validation (no sync needed)")
        invalid_inputs = [
            "invalid_hex",  # not hex
            "ab" * (2 * 1024 * 1024 + 1),  # exceeds max proof size
            "",  # empty string
            "ab" * 32,  # valid hex, too short for a proof
        ]
        for invalid in invalid_inputs:
            with self.assertRaises(HTTPError):
                self.florestad.rpc.verifyutxochaintipinclusionproof(invalid)

        self.log("Mine blocks and sync")
        self.utreexod.rpc.generate(self.NUM_BLOCKS)
        self.connect_nodes(self.florestad, self.utreexod)
        self.wait_for_sync_and_check()

        self.log("Valid proofs")
        coinbase_txids = self.get_coinbase_txids()
        for txid in coinbase_txids:
            proof = self.utreexod.rpc.proveutxochaintipinclusion([txid], [0])
            self.assertTrue(
                self.florestad.rpc.verifyutxochaintipinclusionproof(proof["hex"])
            )

        self.log("Verbosity 1")
        proof = self.utreexod.rpc.proveutxochaintipinclusion([coinbase_txids[0]], [0])
        result = self.florestad.rpc.verifyutxochaintipinclusionproof(proof["hex"], 1)
        self.assertTrue(result["valid"])
        self.assertIn("proved_at_hash", result)
        self.assertIn("targets", result)
        self.assertIn("num_proof_hashes", result)
        self.assertIn("proof_hashes", result)
        self.assertIn("hashes_proven", result)

        self.log("Invalid proofs")
        valid_hex = self.utreexod.rpc.proveutxochaintipinclusion(
            [coinbase_txids[0]], [0]
        )["hex"]

        tampered = valid_hex[:-2] + ("00" if valid_hex[-2:] != "00" else "01")
        self.assertFalse(self.florestad.rpc.verifyutxochaintipinclusionproof(tampered))

        with self.assertRaises(HTTPError):
            self.florestad.rpc.verifyutxochaintipinclusionproof(valid_hex + "ff")

        stale = "00" * 32 + valid_hex[64:]
        with self.assertRaises(HTTPError):
            self.florestad.rpc.verifyutxochaintipinclusionproof(stale)


if __name__ == "__main__":
    VerifyUtxoChainTipInclusionProofTest().main()
