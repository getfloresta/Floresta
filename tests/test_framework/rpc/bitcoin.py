"""
tests.test_framework.rpc.bitcoin.py

A test framework for testing JsonRPC calls to a bitocoin node.
"""

from typing import Optional
from test_framework.rpc.base import BaseRPC


class BitcoinRPC(BaseRPC):
    """
    A class for making RPC calls to a bitcoin-core node.
    """

    def get_jsonrpc_version(self) -> str:
        """
        Get the JSON-RPC version of the node
        """
        return "1.0"

    def generate_block_to_address(self, nblocks: int, address: str) -> list:
        """
        Mine blocks immediately to a specific address by using

        Args:
            nblocks: The number of blocks to mine
            address: The address to mine the blocks to

        Returns:
            A list of block hashes of the newly mined blocks
        """
        return self.perform_request("generatetoaddress", params=[nblocks, address])

    def generate_block(self, nblocks: int) -> list:
        """
        Mine blocks immediately to a address(bcrt1q3ml87jemlfvk7lq8gfs7pthvj5678ndnxnw9ch) using
        `generate_block_to_address(nblocks, address)`

        Args:
            nblocks: The number of blocks to mine

        Returns:
            A list of block hashes of the newly mined blocks
        """
        address = "bcrt1q3ml87jemlfvk7lq8gfs7pthvj5678ndnxnw9ch"
        return self.generate_block_to_address(nblocks, address)

    def get_raw_transaction(
        self, txid: str, verbose: bool, blockhash: Optional[str] = None
    ) -> dict:
        """
        Return the raw transaction data.
        """
        return self.perform_request(
            "getrawtransaction", params=[txid, verbose, blockhash]
        )

    def get_transaction(
        self, txid: str, include_watchonly: bool = True, verbose: bool = False
    ) -> dict:
        """
        Get detailed information about in-wallet transaction
        """
        return self.perform_request(
            "gettransaction",
            params=[txid, include_watchonly, verbose],
        )

    def create_wallet(self, wallet_name: str, *args) -> dict:
        """
        Creates and loads a new wallet.
        """
        return self.perform_request(
            "createwallet",
            params=[wallet_name, *args],
        )

    def get_new_address(
        self, label: Optional[str] = None, address_type: Optional[str] = None
    ) -> dict:
        """
        Returns a new Bitcoin address for receiving payments.
        """
        return self.perform_request("getnewaddress", params=[label, address_type])
