# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getrawtransaction.py

Integrate test for the CLI utility that interacts with a Floresta node
using the `getrawtransaction` RPC method.
"""

import time
import os
import pytest
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

        with pytest.raises(HTTPError):
            self.florestad.rpc.get_raw_transaction("nonexistingtxid")

        with pytest.raises(HTTPError):
            self.florestad.rpc.get_raw_transaction("nonexistingtxid", verbose=0)

        with pytest.raises(HTTPError):
            self.florestad.rpc.get_raw_transaction("nonexistingtxid", verbose=1)

        self.log.info(
            "Test getrawtransaction with a non existing txid and "
            "invalid verbose level"
        )
        with pytest.raises(HTTPError):
            self.florestad.rpc.get_raw_transaction("nonexistingtxid", verbose=2)

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
        bestblockhash = self.bitcoind.rpc.get_bestblockhash()
        block = self.bitcoind.rpc.get_block(bestblockhash)
        txids.append(block["tx"][0])  # Get the coinbase transaction txid

        self.node_manager.connect_nodes(self.bitcoind, utreexod_node)
        time.sleep(5)

        self.node_manager.connect_nodes(self.florestad, utreexod_node)
        time.sleep(5)

        self.node_manager.connect_nodes(self.florestad, self.bitcoind)

        self.log.info("Waiting for Florestad to sync with Bitcoin Core")
        start = time.time()
        blocks = self.bitcoind.rpc.get_block_count()
        while time.time() - start < 30:
            block_chain_info = self.florestad.rpc.get_blockchain_info()
            if block_chain_info["height"] == blocks and not block_chain_info["ibd"]:
                break

            time.sleep(1)

        elapsed_time = time.time() - start
        self.log.info(f"=== time for nodes to sync: {elapsed_time:.2f} seconds ===")
        assert self.florestad.rpc.get_block_count() == blocks
        for txid in txids:
            verbose = 2
            self.log.info(
                f"Testing getrawtransaction for txid: {txid} "
                f"with invalid verbose level {verbose}"
            )
            with pytest.raises(HTTPError):
                self.florestad.rpc.get_raw_transaction(txid, verbose=verbose)

            self.compare_getrawtransaction(txid)

        block_range = (
            f"Testing getrawtransaction for coinbase transactions "
            f"in the block range {blocks - COINBASE_BLOCKS} to {blocks}"
        )
        self.log.info(block_range)
        for height in range(blocks - (COINBASE_BLOCKS - 1), blocks):
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

        self.compare_transaction_data(get_raw_tx, get_raw_tx_bitcoind)

    def compare_transaction_data(self, tx_floresta, tx_bitcoind):
        """Compare transaction data between Floresta and Bitcoin Core."""
        for key in tx_bitcoind.keys():
            self.log.info(f"Comparing key: {key}")

            if key == "vin":
                self.compare_inputs(tx_floresta[key], tx_bitcoind[key])
            elif key == "vout":
                self.compare_outputs(tx_floresta[key], tx_bitcoind[key])
            else:
                assert tx_floresta[key] == tx_bitcoind[key]

    def compare_inputs(self, vin_floresta, vin_bitcoind):
        """Compare transaction inputs (vin) field by field."""
        for i, vin in enumerate(vin_bitcoind):
            self.log.info(f"Comparing vin index: {i}")

            for vin_key in vin.keys():
                self.log.info(f"Comparing vin key: {vin_key}")
                if vin_key == "scriptSig":
                    self.compare_script_sig(vin_floresta[i][vin_key], vin[vin_key])
                else:
                    assert vin_floresta[i][vin_key] == vin[vin_key]

    def compare_script_sig(self, script_sig_floresta, script_sig_bitcoind):
        """Compare scriptSig fields."""
        for script_key in script_sig_bitcoind.keys():
            self.log.info(f"Comparing scriptSig key: {script_key}")
            assert script_sig_floresta[script_key] == script_sig_bitcoind[script_key]

    def compare_outputs(self, vout_floresta, vout_bitcoind):
        """Compare transaction outputs (vout) field by field."""
        for i, vout in enumerate(vout_bitcoind):
            self.log.info(f"Comparing vout index: {i}")

            for vout_key in vout.keys():
                self.log.info(f"Comparing vout key: {vout_key}")

                if vout_key == "scriptPubKey":
                    self.compare_script_pubkey(
                        vout_floresta[i][vout_key], vout[vout_key]
                    )
                else:
                    assert vout_floresta[i][vout_key] == vout[vout_key]

    def compare_script_pubkey(self, spk_floresta, spk_bitcoind):
        """Skip Bitcoin Core specific fields like 'desc'."""
        for spk_key in spk_bitcoind.keys():
            self.log.info(f"Comparing scriptPubKey key: {spk_key}")

            # Skip fields that only Bitcoin Core returns
            # (It's not implemented in Floresta yet)
            if spk_key == "desc":
                continue

            assert spk_floresta[spk_key] == spk_bitcoind[spk_key]
