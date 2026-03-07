"""
tets/test_framework/rpc/utreexo.py

A test framework for testing JsonRPC calls to a utreexo node.
"""

from test_framework.rpc.base import BaseRPC


class UtreexoRPC(BaseRPC):
    """
    A class for making RPC calls to a utreexo node.
    """

    def get_jsonrpc_version(self) -> str:
        """
        Get the JSON-RPC version of the node
        """
        return "1.0"

    def get_new_address(self):
        """
        Perform the `getnewaddress` RPC command to utreexod
        """
        return self.perform_request("getnewaddress", [])

    def generate(self, blocks: int):
        """
        Perform the `generate` RPC command to utreexod.
        """
        return self.perform_request("generate", [blocks])

    def get_utreexo_roots(self, block_hash: str):
        """
        Perform the `getutreexoroots` RPC command to utreexod
        """
        return self.perform_request("getutreexoroots", [block_hash])

    def invalidate_block(self, blockhash: str):
        """
        Invalidate a block by its hash performing
        `perform_request('invalidateblock', params=[<str>])`
        """
        return self.perform_request("invalidateblock", params=[blockhash])

    def addnode(
        self, node: str, command: str, v2transport: bool = False, rpcquirk: bool = False
    ):
        """
        Adds a new node to our list of peers performing
        `perform_request('addnode', params=[str])`

        This will make our node try to connect to this peer.

        Args
            node: A network address with the format ip[:port]

        Returns
            success: Whether we successfully added this node to our list of peers
        """
        if rpcquirk:
            return self.perform_request("addnode", params=[node, command, v2transport])

        return self.perform_request("addnode", params=[node, command])

    def proveutxochaintipinclusion(self, txids: list, vouts: list) -> dict:
        """
        Generate a Utreexo proof for UTXOs at the current chain tip.
        """
        return self.perform_request("proveutxochaintipinclusion", [txids, vouts])
