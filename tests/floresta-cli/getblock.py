"""
floresta_cli_getblock.py

This functional test cli utility to interact with a Floresta node with `getblock`
"""

import time
import random

from test_framework import FlorestaTestFramework

DATA_DIR = FlorestaTestFramework.get_integration_test_dir()


class GetBlockTest(FlorestaTestFramework):

    def set_test_params(self):

        self.v2transport = True
        self.data_dirs = GetBlockTest.create_data_dirs(DATA_DIR, "get_block", 2)

        self.florestad = self.add_node(
            variant="florestad",
            extra_args=[f"--data-dir={self.data_dirs[0]}"],
        )

        self.bitcoind = self.add_node(
            variant="bitcoind",
            extra_args=[f"-datadir={self.data_dirs[1]}", "-v2transport=1"],
        )

    def compare_block(self, height: int):
        block_hash = self.bitcoind.rpc.get_blockhash(height)
        self.log(f"Comparing block {block_hash} between florestad and bitcoind")

        self.log("Fetching request with verbosity 0")
        floresta_block = self.florestad.rpc.get_block(block_hash, 0)
        bitcoind_block = self.bitcoind.rpc.get_block(block_hash, 0)
        self.assertEqual(floresta_block, bitcoind_block)

        self.log("Fetching request with verbosity 1")
        floresta_block = self.florestad.rpc.get_block(block_hash, 1)
        bitcoind_block = self.bitcoind.rpc.get_block(block_hash, 1)

        for key, bval in bitcoind_block.items():
            fval = floresta_block[key]

            self.log(f"Comparing {key} field: floresta={fval} bitcoind={bval}")
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
        bitcoind_port = self.bitcoind.get_port("p2p")
        self.florestad.rpc.addnode(
            node=f"127.0.0.1:{bitcoind_port}",
            command="add",
            v2transport=self.v2transport,
        )

        block_count = self.bitcoind.rpc.get_block_count()
        end = time.time() + 20
        while time.time() < end:
            if self.florestad.rpc.get_block_count() == block_count:
                break
            time.sleep(0.5)

        self.assertEqual(
            self.florestad.rpc.get_block_count(), self.bitcoind.rpc.get_block_count()
        )

        self.log("Testing getblock RPC in the genesis block")
        self.compare_block(0)

        random_block = random.randint(1, block_count)
        self.log(f"Testing getblock RPC in block {random_block}")
        self.compare_block(random_block)

        self.log(f"Testing getblock RPC in block {block_count}")
        self.compare_block(block_count)


if __name__ == "__main__":
    GetBlockTest().main()
