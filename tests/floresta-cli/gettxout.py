"""
gettxout.py

This functional test cli utility to interact with a Floresta node with `getxtout` command.
"""

import re
import time
import os
from test_framework import FlorestaTestFramework

DATA_DIR = FlorestaTestFramework.get_integration_test_dir()

# TODO Use many addresses types as possible to test the gettxout command
WALLET_CONFIG = "\n".join(
    [
        "[wallet]",
        'addresses = [ "bcrt1q4gfcga7jfjmm02zpvrh4ttc5k7lmnq2re52z2y" ]',
    ]
)


class GetTxoutTest(FlorestaTestFramework):
    """
    Test `gettxout` command in Floresta compared with the expected output in bitcoin-core.
    """

    def set_test_params(self):
        """
        Setup floresta, utreexod, and bitcoind nodes with their respective data directories.
        Also create a config.toml file for the floresta wallet so we can track the address.
        """
        name = self.__class__.__name__.lower()
        config_path = os.path.join(DATA_DIR, "data", name, "config.toml")

        self.data_dirs = self.create_data_dirs(DATA_DIR, name, 3)

        with open(config_path, "w") as f:
            f.write(WALLET_CONFIG)

        self.florestad = self.add_node(
            variant="florestad",
            extra_args=[
                f"--config-file={config_path}",
                f"--data-dir={self.data_dirs[0]}",
            ],
        )

        self.utreexod = self.add_node(
            variant="utreexod",
            extra_args=[
                f"--datadir={self.data_dirs[1]}",
                "--miningaddr=bcrt1q4gfcga7jfjmm02zpvrh4ttc5k7lmnq2re52z2y",
                "--prune=0",
            ],
        )

        self.bitcoind = self.add_node(
            variant="bitcoind", extra_args=[f"-datadir={self.data_dirs[2]}"]
        )

    # TODO create and sign some transactions to test the gettxout command
    def run_test(self):
        """
        Run JSONRP and get the hash of height 0
        """
        # Start node
        self.run_node(self.florestad)
        self.run_node(self.utreexod)
        self.run_node(self.bitcoind)

        self.log("=== Mining blocks with utreexod")
        self.utreexod.rpc.generate(10)
        time.sleep(5)

        self.log("=== Connect floresta to utreexod")
        host = self.utreexod.get_host()
        port = self.utreexod.get_port("p2p")
        self.florestad.rpc.addnode(
            f"{host}:{port}", command="onetry", v2transport=False
        )

        self.log("=== Waiting for floresta to connect to utreexod...")
        self.wait_for_peers_connections(self.florestad, self.utreexod)

        self.log("=== Connect bitcoind to utreexod")
        host = self.utreexod.get_host()
        port = self.utreexod.get_port("p2p")
        self.bitcoind.rpc.addnode(f"{host}:{port}", command="onetry", v2transport=False)

        self.log("=== Waiting for bitcoind to connect to utreexod...")
        self.wait_for_peers_connections(self.bitcoind, self.utreexod)

        self.log("=== Wait for the nodes to sync...")
        time.sleep(5)

        self.log("=== Get a list of transactions")
        blocks = self.florestad.rpc.get_block_count()
        for height in range(1, blocks):
            self.log(f"=== Getting block at height {height}")
            block_hash = self.florestad.rpc.get_blockhash(height)

            block = self.florestad.rpc.get_block(block_hash)

            for tx in block["tx"]:
                self.log(f"=== Getting txout for tx {tx}")
                txout_floresta = self.florestad.rpc.get_txout(
                    tx, vout=0, include_mempool=False
                )
                txout_bitcoind = self.bitcoind.rpc.get_txout(
                    tx, vout=0, include_mempool=False
                )

                for key in ("bestblock", "coinbase", "value", "confirmations"):
                    self.assertEqual(txout_floresta[key], txout_bitcoind[key])

                for key in ("address", "desc", "hex", "type", "asm"):
                    self.assertEqual(
                        txout_floresta["scriptPubKey"][key],
                        txout_bitcoind["scriptPubKey"][key],
                    )


if __name__ == "__main__":
    GetTxoutTest().main()
