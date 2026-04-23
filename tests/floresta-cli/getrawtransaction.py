# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getrawtransaction.py

This is a functional test for the CLI utility that interacts with a Floresta node using `getrawtransaction`.
"""

import re
import time
import os
from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from requests.exceptions import HTTPError

ADDRESS_COINBASE = "bcrt1q4gfcga7jfjmm02zpvrh4ttc5k7lmnq2re52z2y"
ADDRESS_LEGACY = "n2eoQNSGg7ZWjnbXzdnGDMHZShn3MjaEfR"
ADDRESS_P2PKH = "mnh5HKWqsYdRo8ChUc2rN9bDcMRj4HopFw"
ADDRESS_P2PWH = "2NG7vNNXVRMihLD14WBbxhMxEQ1qKBkepq1"
ADDRESS_BECH32 = "bcrt1q427ze5mrzqupzyfmqsx9gxh7xav538yk2j4cft"
ADDRESS_BECH32M = "bcrt1p929uxzkp0lnh3smkkcvdqerj7ejhhac6vsc2lr0gqc4l092w8yjq64dhfy"
ADDRESS_TAPROOT = "bcrt1pnmrmugapastum8ztvgwcn8hvq2avmcwh2j4ssru7rtyygkpqq98q4wyd6s"

WALLET_CONFIG = "\n".join(
    [
        "[wallet]",
        f'addresses = [ "{ADDRESS_COINBASE}", "{ADDRESS_LEGACY}", "{ADDRESS_P2PKH}", "{ADDRESS_P2PWH}", "{ADDRESS_BECH32}", "{ADDRESS_BECH32M}", "{ADDRESS_TAPROOT}" ]',
    ]
)

MINED_BLOCKS = 101
COINBASE_BLOCKS = 6


class GetRawTransactionTest(FlorestaTestFramework):
    """
    Test `getrawtransaction` RPC method of Floresta node by comparing its response
    with Bitcoin Core's response for the same transaction.
    """

    def set_test_params(self):
        """
        Setup `bitcoind`, `florestad`, and `utreexod` in the same regtest network.
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

        self.bitcoind = self.add_node_extra_args(
            variant=NodeType.BITCOIND,
            extra_args=["-txindex", "-fallbackfee=0.00000001"],
        )

        self.utreexod = self.add_node_default_args(variant=NodeType.UTREEXOD)

    def run_test(self):
        """
        Run the test by sending transactions, mining blocks, and comparing `getrawtransaction`
        responses between Floresta and Bitcoin Core.
        """
        self.run_node(self.florestad)

        self.log("Test getrawtransaction with a non existing txid")

        with self.assertRaises(HTTPError) as e:
            self.florestad.rpc.get_raw_transaction("nonexistingtxid")

        with self.assertRaises(HTTPError) as e:
            self.florestad.rpc.get_raw_transaction("nonexistingtxid", verbose=0)

        with self.assertRaises(HTTPError) as e:
            self.florestad.rpc.get_raw_transaction("nonexistingtxid", verbose=1)

        self.log(
            "Test getrawtransaction with a non existing txid and invalid verbose level"
        )
        with self.assertRaises(HTTPError) as e:
            self.florestad.rpc.get_raw_transaction("nonexistingtxid", verbose=2)

        self.run_node(self.bitcoind)
        self.bitcoind.rpc.create_wallet("testwallet")

        self.bitcoind.rpc.generate_block_to_wallet(MINED_BLOCKS)

        coinbase_txid = []
        value = 6.4242521
        for address in [
            ADDRESS_BECH32,
            ADDRESS_LEGACY,
            ADDRESS_BECH32M,
            ADDRESS_P2PWH,
            ADDRESS_P2PKH,
            ADDRESS_TAPROOT,
        ]:
            txid = self.bitcoind.rpc.send_to_address(address, value)
            self.log(
                f"Sent transaction to {address} and value {value} with txid: {txid}"
            )
            coinbase_txid.append(txid)

        txid = self.bitcoind.rpc.send_to_address(ADDRESS_BECH32, 5.5)
        self.log(f"Sent transaction with txid: {txid}")
        self.bitcoind.rpc.generate_block_to_address(COINBASE_BLOCKS, ADDRESS_COINBASE)

        self.run_node(self.utreexod)

        self.connect_nodes(self.bitcoind, self.utreexod)
        time.sleep(5)

        self.connect_nodes(self.florestad, self.utreexod)
        time.sleep(5)

        self.connect_nodes(self.florestad, self.bitcoind)

        self.log("Waiting for Florestad to sync with Bitcoin Core")
        start = time.time()
        blocks = self.bitcoind.rpc.get_block_count()
        while time.time() - start < 30:
            block_chain_info = self.florestad.rpc.get_blockchain_info()
            if block_chain_info["height"] == blocks and not block_chain_info["ibd"]:
                break

            time.sleep(1)

        self.log(
            "=== time for nodes to sync: {:.2f} seconds ===".format(time.time() - start)
        )
        self.assertEqual(self.florestad.rpc.get_block_count(), blocks)
        for txid in coinbase_txid:
            verbose = 2
            self.log(
                f"Testing getrawtransaction for txid: {txid} with invalid verbose level {verbose}"
            )
            with self.assertRaises(HTTPError) as e:
                self.florestad.rpc.get_raw_transaction(txid, verbose=verbose)

            self.compare_getrawtransaction(txid)

        self.log(
            f"Testing getrawtransaction for coinbase transactions in the block range {blocks - COINBASE_BLOCKS} to {blocks}"
        )
        for height in range(blocks - (COINBASE_BLOCKS - 1), blocks):
            block_hash = self.bitcoind.rpc.get_blockhash(height)

            block = self.bitcoind.rpc.get_block(block_hash, verbosity=1)
            coinbase_txid = block["tx"][0]  # Get the coinbase transaction txid

            self.compare_getrawtransaction(coinbase_txid)

    def compare_getrawtransaction(self, txid):
        self.log(f"Comparing getrawtransaction for txid: {txid} with verbose level 0")
        get_raw_tx = self.florestad.rpc.get_raw_transaction(txid, verbose=0)
        get_raw_tx_bitcoind = self.bitcoind.rpc.get_raw_transaction(txid, verbose=0)
        self.assertEqual(get_raw_tx, get_raw_tx_bitcoind)

        self.log(f"Comparing getrawtransaction for txid: {txid} with verbose level 1")
        get_raw_tx = self.florestad.rpc.get_raw_transaction(txid, verbose=1)
        get_raw_tx_bitcoind = self.bitcoind.rpc.get_raw_transaction(txid, verbose=1)

        self.compare_transaction_data(get_raw_tx, get_raw_tx_bitcoind)

    def compare_transaction_data(self, tx_floresta, tx_bitcoind):
        """Compare all transaction data between Floresta and Bitcoin Core responses"""
        for key in tx_bitcoind.keys():
            self.log(f"Comparing key: {key}")

            if key == "vin":
                self.compare_inputs(tx_floresta[key], tx_bitcoind[key])
            elif key == "vout":
                self.compare_outputs(tx_floresta[key], tx_bitcoind[key])
            else:
                self.assertEqual(tx_floresta[key], tx_bitcoind[key])

    def compare_inputs(self, vin_floresta, vin_bitcoind):
        """Compare transaction inputs (vin) field by field"""
        for i, vin in enumerate(vin_bitcoind):
            self.log(f"Comparing vin index: {i}")

            for vin_key in vin.keys():
                self.log(f"Comparing vin key: {vin_key}")
                if vin_key == "scriptSig":
                    self.compare_script_sig(
                        vin_floresta[i][vin_key], vin_bitcoind[i][vin_key]
                    )
                else:
                    self.assertEqual(vin_floresta[i][vin_key], vin_bitcoind[i][vin_key])

    def compare_script_sig(self, script_sig_floresta, script_sig_bitcoind):
        """Compare scriptSig fields"""
        for script_key in script_sig_bitcoind.keys():
            self.log(f"Comparing scriptSig key: {script_key}")
            self.assertEqual(
                script_sig_floresta[script_key], script_sig_bitcoind[script_key]
            )

    def compare_outputs(self, vout_floresta, vout_bitcoind):
        """Compare transaction outputs (vout) field by field"""
        for i, vout in enumerate(vout_bitcoind):
            self.log(f"Comparing vout index: {i}")

            for vout_key in vout.keys():
                self.log(f"Comparing vout key: {vout_key}")

                if vout_key == "scriptPubKey":
                    self.compare_script_pubkey(
                        vout_floresta[i][vout_key], vout_bitcoind[i][vout_key]
                    )
                else:
                    self.assertEqual(
                        vout_floresta[i][vout_key], vout_bitcoind[i][vout_key]
                    )

    def compare_script_pubkey(self, spk_floresta, spk_bitcoind):
        """Compare scriptPubKey fields, skipping Bitcoin Core specific fields like 'desc'"""
        for spk_key in spk_bitcoind.keys():
            self.log(f"Comparing scriptPubKey key: {spk_key}")

            # Skip fields that only Bitcoin Core returns (It's not implemented in Floresta yet)
            if spk_key == "desc":
                continue

            self.assertEqual(spk_floresta[spk_key], spk_bitcoind[spk_key])


if __name__ == "__main__":
    GetRawTransactionTest().main()
