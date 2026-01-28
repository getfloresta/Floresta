"""
tests.test_framework.rpc.bitcoin.py

A test framework for testing JsonRPC calls to a bitocoin node.
"""

from test_framework.rpc.base import BaseRPC
from test_framework.constants import WALLET_ADDRESS


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
        Mine blocks immediately to a address(WALLET_ADDRESS) using
        `generate_block_to_address(nblocks, address)`

        Args:
            nblocks: The number of blocks to mine

        Returns:
            A list of block hashes of the newly mined blocks
        """
        return self.generate_block_to_address(nblocks, WALLET_ADDRESS)
