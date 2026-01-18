"""
tests.test_framework.rpc.bitcoin.py

A test framework for testing JsonRPC calls to a bitocoin node.
"""

import re
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

    def get_blockheader(self, blockhash: str) -> dict:
        """
        Get the header of a block, giving its hash performing
        `perform_request('getblockheader', params=[<str>])`
        """
        if not bool(re.fullmatch(r"^[a-f0-9]{64}$", blockhash)):
            raise ValueError(f"Invalid blockhash '{blockhash}'.")

        return self.perform_request("getblockheader", params=[blockhash])

    # pylint: disable=R0801
    def get_block(self, blockhash: str, verbosity: int = 1):
        """
        Get a full block, given its hash performing
        `perform_request('getblock', params=[str, int])`

        Notice that this rpc will cause a actual network request to our node,
        so it may be slow, and if used too often, may cause more network usage.
        The returns for this rpc are identical to bitcoin core's getblock rpc
        as of version 27.0.

        the `str` param should be a valid 32 bytes hex formatted string
        the `int` param should be a integer verbosity level
        """
        if len(blockhash) != 64:
            raise ValueError(f"invalid blockhash param: {blockhash}")

        if verbosity not in (0, 1):
            raise ValueError(f"Invalid verbosity level param: {verbosity}")

        return self.perform_request("getblock", params=[blockhash, verbosity])

    # pylint: disable=R0801
    def addnode(self, node: str, command: str, v2transport: bool = False):
        """
        Adds a new node to our list of peers performing
        `perform_request('addnode', params=[str])`

        This will make our node try to connect to this peer.

        Args
            node: A network address with the format ip[:port]

        Returns
            success: Whether we successfully added this node to our list of peers
        """
        # matches, IPv4, IPv6 and optional ports from 0 to 65535
        pattern = re.compile(
            r"^("
            r"(?:(?:25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\.){3}"
            r"(?:25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])|"
            r"\[([a-fA-F0-9:]+)\]"
            r")"
            r"(:(6553[0-5]|655[0-2][0-9]|65[0-4][0-9]{2}|6[0-4][0-9]{3}|[1-9]?[0-9]{1,4}))?$"
        )

        if not pattern.match(node):
            raise ValueError("Invalid ip[:port] format")

        if command not in ("add", "remove", "onetry"):
            raise ValueError(f"Invalid command '{command}'")

        return self.perform_request("addnode", params=[node, command, v2transport])
