# SPDX-License-Identifier: MIT OR Apache-2.0

"""
tests.test_framework.rpc.bitcoin.py

A test framework for testing JsonRPC calls to a bitocoin node.
"""

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

    def get_chain_tips(self):
        """Returns information about all known chain tips in the block tree"""
        return self.perform_request("getchaintips")

    def invalidate_block(self, blockhash: str):
        """Marks a block as invalid"""
        return self.perform_request("invalidateblock", params=[blockhash])

    def submit_header(self, hexdata: str):
        """Submits a raw block header as a candidate chain tip"""
        return self.perform_request("submitheader", params=[hexdata])
