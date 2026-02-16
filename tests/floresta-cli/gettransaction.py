"""
gettransaction.py

This functional test cli utility to interact with a Floresta node with `getxtout` command.
"""

import re
import time
import os
from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class GetTransactionTest(FlorestaTestFramework):
    """
    Test `gettransaction` command in Floresta compared with the expected output in bitcoin-core.
    """

    def set_test_params(self):
        """
        Setup floresta, utreexod, and bitcoind nodes with their respective data directories.
        Also create a config.toml file for the floresta wallet so we can track the address.
        """
        name = self.__class__.__name__.lower()
        data_dir = self.create_data_dir_for_daemon(NodeType.FLORESTAD)
        config_path = os.path.join(data_dir, "config.toml")

        self.bitcoind = self.add_node_extra_args(
            variant=NodeType.BITCOIND,
            extra_args=["-txindex"],
        )

        self.log("=== Starting bitcoind node")
        self.run_node(self.bitcoind)

        self.log("=== Create bitcoind wallet")
        self.bitcoind.rpc.create_wallet(name)

        self.log("=== Generate new address")
        wallet_addr = self.bitcoind.rpc.get_new_address(address_type="bech32")

        with open(config_path, "w") as f:
            f.write(
                "\n".join(
                    [
                        "[wallet]",
                        f'addresses = [ "{wallet_addr}" ]',
                    ]
                )
            )

        self.florestad = self.add_node_extra_args(
            variant=NodeType.FLORESTAD,
            extra_args=[
                f"--config-file={config_path}",
            ],
        )

        self.log("=== Starting florestad node")
        self.run_node(self.florestad)

        self.utreexod = self.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                f"--miningaddr={wallet_addr}",
                "--prune=0",
            ],
        )

        self.log("=== Starting utreexod node")
        self.run_node(self.utreexod)

    def run_test(self):
        """
        Run JSONRPC and get the hash of height 0
        """
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

            self.log(f"=== Checking if gettransaction is compatible with Bitcoin-Core")
            for tx in block["tx"]:
                transaction_floresta = self.florestad.rpc.get_transaction(
                    tx, verbosity=True
                )
                transaction_bitcoind = self.bitcoind.rpc.get_transaction(
                    tx, verbose=True
                )

                for key in ["version", "size", "vsize", "weight", "locktime"]:
                    self.assertEqual(
                        transaction_floresta[key], transaction_bitcoind["decoded"][key]
                    )

                for i in range(len(transaction_floresta["vin"])):
                    self.assertEqual(
                        transaction_floresta["vin"][i]["sequence"],
                        transaction_bitcoind["decoded"]["vin"][i]["sequence"],
                    )

                for i in range(len(transaction_floresta["vout"])):
                    self.assertEqual(
                        transaction_floresta["vout"][i]["n"],
                        transaction_bitcoind["decoded"]["vout"][i]["n"],
                    )
                    self.assertEqual(
                        transaction_floresta["vout"][i]["script_pub_key"]["address"],
                        transaction_bitcoind["decoded"]["vout"][i]["scriptPubKey"][
                            "address"
                        ],
                    )
                    self.assertEqual(
                        transaction_floresta["vout"][i]["script_pub_key"]["hex"],
                        transaction_bitcoind["decoded"]["vout"][i]["scriptPubKey"][
                            "hex"
                        ],
                    )

            for tx in block["tx"]:
                transaction_floresta = self.florestad.rpc.get_transaction(
                    tx, verbosity=False
                )
                transaction_bitcoind = self.bitcoind.rpc.get_transaction(
                    tx, verbose=False
                )

                self.assertEqual(
                    transaction_floresta["hex"], transaction_bitcoind["hex"]
                )


if __name__ == "__main__":
    GetTransactionTest().main()
