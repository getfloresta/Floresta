"""
Test verifyutxochaintipinclusionproof RPC.
"""

from test_framework import FlorestaTestFramework
from requests.exceptions import HTTPError
import shutil
import os
import time

DATA_DIR = FlorestaTestFramework.get_integration_test_dir()
MINING_ADDR = "bcrt1p5q9rq6tja29u2dafp0j7a2ul6qtuuna62m7ky8f6yqrvwf5j7knqmv2lus"


class VerifyUtxoChainTipInclusionProofTest(FlorestaTestFramework):
    """
    Test verifyutxochaintipinclusionproof RPC with valid and invalid inputs.
    """

    NUM_BLOCKS = 10

    def set_test_params(self):
        name = self.__class__.__name__.lower()
        self.data_dirs = self.create_data_dirs(DATA_DIR, name, 2)

        for d in self.data_dirs:
            if os.path.exists(d):
                shutil.rmtree(d)
            os.makedirs(d, exist_ok=True)

        self.florestad = self.add_node(
            variant="florestad",
            extra_args=[f"--data-dir={self.data_dirs[0]}"],
        )
        self.utreexod = self.add_node(
            variant="utreexod",
            extra_args=[
                f"--datadir={self.data_dirs[1]}",
                "--flatutreexoproofindex",
                "--cfilters",
                "--prune=0",
                f"--miningaddr={MINING_ADDR}",
            ],
        )

    def wait_for_sync_and_check(self):
        """Poll until Floresta has validated up to the given height."""
        height = self.utreexod.rpc.get_block_count()
        end = time.time() + 60
        while time.time() < end:
            info = self.florestad.rpc.get_blockchain_info()
            if info["validated"] == height and not info["ibd"]:
                break
            time.sleep(1)

        self.assertEqual(info["validated"], height)
        self.assertFalse(info["ibd"])

    def get_coinbase_txids(self):
        """Retrieve coinbase transaction IDs for the first `count` blocks."""
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

        host = self.utreexod.get_host()
        port = self.utreexod.get_port("p2p")
        self.florestad.rpc.addnode(
            f"{host}:{port}", command="onetry", v2transport=False
        )
        self.wait_for_peers_connections(self.florestad, self.utreexod)

        self.wait_for_sync_and_check()

        self.log("Valid proofs")
        coinbase_txids = self.get_coinbase_txids()
        for txid in coinbase_txids:
            proof = self.utreexod.rpc.proveutxochaintipinclusion([txid], [0])
            self.assertTrue(
                self.florestad.rpc.verifyutxochaintipinclusionproof(proof["hex"])
            )

        self.log("Invalid proofs")
        valid_hex = self.utreexod.rpc.proveutxochaintipinclusion(
            [coinbase_txids[0]], [0]
        )["hex"]

        # Tampered last byte - verification returns False
        tampered = valid_hex[:-2] + ("00" if valid_hex[-2:] != "00" else "01")
        self.assertFalse(self.florestad.rpc.verifyutxochaintipinclusionproof(tampered))

        # Trailing bytes - decode error
        with self.assertRaises(HTTPError):
            self.florestad.rpc.verifyutxochaintipinclusionproof(valid_hex + "ff")

        # Wrong block hash - stale proof error
        stale = "00" * 32 + valid_hex[64:]
        with self.assertRaises(HTTPError):
            self.florestad.rpc.verifyutxochaintipinclusionproof(stale)


if __name__ == "__main__":
    VerifyUtxoChainTipInclusionProofTest().main()
