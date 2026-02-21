"""
gettxout.py

This functional test cli utility to interact with a Floresta node with `getxtout` command.
"""

import re
import time
import os
from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.constants import WALLET_ADDRESS

# TODO Use many addresses types as possible to test the gettxout command
WALLET_CONFIG = "\n".join(
    [
        "[wallet]",
        f'addresses = [ "{WALLET_ADDRESS}" ]',
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
        data_dir = self.create_data_dir_for_daemon(NodeType.FLORESTAD)
        config_path = os.path.join(data_dir, "config.toml")

        with open(config_path, "w") as f:
            f.write(WALLET_CONFIG)

        self.florestad = self.add_node_extra_args(
            variant=NodeType.FLORESTAD,
            extra_args=[
                f"--config-file={config_path}",
            ],
        )

        self.utreexod = self.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                f"--miningaddr={WALLET_ADDRESS}",
                "--prune=0",
            ],
        )

        self.bitcoind = self.add_node_default_args(
            variant=NodeType.BITCOIND,
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
        self.connect_nodes(self.florestad, self.utreexod)

        self.log("=== Connect bitcoind to utreexod")
        self.connect_nodes(self.bitcoind, self.utreexod)

        self.log("=== Wait for the nodes to sync...")
        end = time.time() + 20
        while time.time() < end:
            if (
                self.florestad.rpc.get_block_count()
                == self.bitcoind.rpc.get_block_count()
                == self.utreexod.rpc.get_block_count()
            ) and not self.florestad.rpc.get_blockchain_info()["ibd"]:
                break

            time.sleep(1)

        self.assertFalse(self.florestad.rpc.get_blockchain_info()["ibd"])

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
