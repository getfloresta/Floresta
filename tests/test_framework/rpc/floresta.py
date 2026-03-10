# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_rpc.py

A test framework for testing JsonRPC calls to a floresta node.
"""

from test_framework.rpc.base import BaseRPC


class FlorestaRPC(BaseRPC):
    """
    A class for making RPC calls to a floresta node.
    """

    def get_jsonrpc_version(self) -> str:
        """
        Get the JSON-RPC version of the node
        """
        return "2.0"

    def get_roots(self):
        """
        Returns the roots of our current floresta state performing
        """
        return self.perform_request("getroots")

    def invalidate_block(self, blockhash: str):
        """Marks a block as invalid"""
        return self.perform_request("invalidateblock", params=[blockhash])

    def submit_header(self, hexdata: str):
        """Submits a raw block header as a candidate chain tip"""
        return self.perform_request("submitheader", params=[hexdata])

    def get_memoryinfo(self, mode: str):
        """
        Returns stats about our memory usage performing
        """
        if mode not in ("stats", "mallocinfo"):
            raise ValueError(f"Invalid getmemoryinfo mode: '{mode}'")

        return self.perform_request("getmemoryinfo", params=[mode])
