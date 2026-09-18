# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getrawtransaction.py

Integrate test for the CLI utility that interacts with a Floresta node
using the `getrawtransaction` RPC method.
"""

import os
import pytest
from test_framework.node import NodeType
from test_framework.util import compare_fields

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
        (
            f'addresses = [ "{ADDRESS_COINBASE}", '
            f'"{ADDRESS_LEGACY}", "{ADDRESS_P2PKH}", '
            f'"{ADDRESS_P2PWH}", "{ADDRESS_BECH32}", '
            f'"{ADDRESS_BECH32M}", "{ADDRESS_TAPROOT}" ]'
        ),
    ]
)

MINED_BLOCKS = 101
COINBASE_BLOCKS = 6

SPECIAL_FIELDS = ["vin", "vout", "scriptSig", "scriptPubKey"]
IGNORE_FIELDS = ["desc"]


class TestGetRawTransaction:
    """
    Test `getrawtransaction` RPC method of Floresta node by comparing
    its response with Bitcoin Core's response for the same transaction.
    """

    log = None
    node_manager = None
    florestad = None
    bitcoind = None

    # pylint: disable=too-many-statements,too-many-locals
    @pytest.mark.rpc
    def test_get_raw_transaction(
        self, setup_logging, node_manager, add_node_with_extra_args, utreexod_node
    ):
        """
        Run the test by sending transactions, mining blocks, and
        comparing `getrawtransaction` responses between Floresta
        and Bitcoin Core.
        """
        self.log = setup_logging
        self.node_manager = node_manager

        self.florestad = node_manager.add_node_default_args(variant=NodeType.FLORESTAD)
        config_dir = os.path.join(self.florestad.daemon.data_dir, "config.toml")
        with open(config_dir, "w", encoding="utf-8") as f:
            f.write(WALLET_CONFIG)
            self.florestad.set_extra_args([f"--config-file={config_dir}"])

        node_manager.run_node(self.florestad)

        self.log.info("Test getrawtransaction with a non existing txid")

        self.florestad.rpc.ensure_rpc_call_error(
            method="getrawtransaction",
            params=["nonexistingtxid"],
        )

        self.florestad.rpc.ensure_rpc_call_error(
            method="getrawtransaction",
            params=["nonexistingtxid", 0],
        )

        self.florestad.rpc.ensure_rpc_call_error(
            method="getrawtransaction",
            params=["nonexistingtxid", 1],
        )

        self.log.info("Creating and funding transactions in Bitcoin Core")
        self.bitcoind = add_node_with_extra_args(
            variant=NodeType.BITCOIND,
            extra_args=["-txindex", "-fallbackfee=0.00000001"],
        )
        self.bitcoind.rpc.create_wallet("testwallet")

        self.bitcoind.rpc.generate_block_to_wallet(MINED_BLOCKS)

        txids = []
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
            self.log.info(
                f"Sent transaction to {address} and value {value} " f"with txid: {txid}"
            )
            txids.append(txid)

        txids.append(self.bitcoind.rpc.send_to_address(ADDRESS_BECH32, 5.5))
        self.log.info(f"Sent transaction with txid: {txid}")

        self.bitcoind.rpc.generate_block_to_address(COINBASE_BLOCKS, ADDRESS_COINBASE)

        self.node_manager.connect_nodes(self.bitcoind, utreexod_node)
        self.node_manager.connect_nodes(self.florestad, utreexod_node)
        self.node_manager.connect_nodes(self.florestad, self.bitcoind)

        self.log.info("Waiting for Florestad to sync with Bitcoin Core")
        self.node_manager.wait_for_sync_nodes()

        self.log.info(
            "Test getrawtransaction with a non existing txid and "
            "invalid verbose level"
        )
        self.florestad.rpc.ensure_rpc_call_error(
            method="getrawtransaction",
            params=[txids[0], 2],
        )

        for txid in txids:
            self.compare_getrawtransaction(txid)

        initial_block_coinbase_wallet = self.bitcoind.rpc.get_block_count() - (
            COINBASE_BLOCKS - 1
        )
        best_block_height = self.bitcoind.rpc.get_block_count()
        block_range = (
            f"Testing getrawtransaction for coinbase transactions "
            f"in the block range {initial_block_coinbase_wallet} to {best_block_height}"
        )
        self.log.info(block_range)
        for height in range(initial_block_coinbase_wallet, best_block_height):
            block_hash = self.bitcoind.rpc.get_blockhash(height)

            block = self.bitcoind.rpc.get_block(block_hash, verbosity=1)
            coinbase_tx = block["tx"][0]  # Get the coinbase transaction txid

            self.compare_getrawtransaction(coinbase_tx)

    def compare_getrawtransaction(self, txid):
        """Compare getrawtransaction output between Floresta and Bitcoin Core."""
        self.log.info(
            f"Comparing getrawtransaction for txid: {txid} with verbose default (verbose=0)"
        )
        get_raw_tx = self.florestad.rpc.get_raw_transaction(txid)
        get_raw_tx_bitcoind = self.bitcoind.rpc.get_raw_transaction(txid)
        assert get_raw_tx == get_raw_tx_bitcoind

        self.log.info(
            f"Comparing getrawtransaction for txid: {txid} with verbose level 0"
        )
        get_raw_tx = self.florestad.rpc.get_raw_transaction(txid, verbose=0)
        get_raw_tx_bitcoind = self.bitcoind.rpc.get_raw_transaction(txid, verbose=0)
        assert get_raw_tx == get_raw_tx_bitcoind

        self.log.info(
            f"Comparing getrawtransaction for txid: {txid} with verbose level 1"
        )
        get_raw_tx = self.florestad.rpc.get_raw_transaction(txid, verbose=1)
        get_raw_tx_bitcoind = self.bitcoind.rpc.get_raw_transaction(txid, verbose=1)

        compare_fields(get_raw_tx, get_raw_tx_bitcoind, ignore_fields=IGNORE_FIELDS)
