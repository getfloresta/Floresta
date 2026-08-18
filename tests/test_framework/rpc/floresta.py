# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_rpc.py

A test framework for testing JsonRPC calls to a floresta node.
"""

from typing import Optional

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

    def get_memoryinfo(self, mode: str):
        """
        Returns stats about our memory usage performing
        """
        if mode not in ("stats", "mallocinfo"):
            raise ValueError(f"Invalid getmemoryinfo mode: '{mode}'")

        return self.perform_request("getmemoryinfo", params=[mode])

    def load_descriptor(self, descriptor: str):
        """
        Load a script descriptor into the wallet.
        """
        return self.perform_request("loaddescriptor", params=[descriptor])

    def get_added_node_info(self, node: Optional[str] = None) -> list:
        """
        Get information about manually added nodes.
        node: optional filter — return only this added node's info
        """
        params = [node] if node is not None else []
        return self.perform_request("getaddednodeinfo", params)
