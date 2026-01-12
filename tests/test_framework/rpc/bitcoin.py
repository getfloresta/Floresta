"""
tests.test_framework.rpc.bitcoin.py

A test framework for testing JsonRPC calls to a bitocoin node.
"""

import re
from test_framework.rpc.base import BaseRPC

REGTEST_RPC_SERVER = {
    "host": "127.0.0.1",
    "ports": {"rpc": 18443, "p2p": 18444},
    "user": "bitcoin",
    "password": "bitcoin",
    "jsonrpc": "2.0",
    "timeout": 10000,
}


class BitcoinRPC(BaseRPC):
    """
    A class for making RPC calls to a bitcoin-core node.
    """

    def get_blockchain_info(self) -> dict:
        """
        Get the blockchain info by performing `perform_request('getblockchaininfo')`
        """
        return self.perform_request("getblockchaininfo")

    def get_blockhash(self, height: int) -> dict:
        """
        Get the blockhash associated with a given height performing
        `perform_request('getblockhash', params=[<int>])`
        """
        return self.perform_request("getblockhash", [height])

    # pylint: disable=R0801
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
    def get_bestblockhash(self) -> str:
        """
        Get the hash of the best block in the chain performing
        `perform_request('getbestblockhash')`
        """
        return self.perform_request("getbestblockhash")

    # pylint: disable=R0801
    def get_block_count(self) -> int:
        """
        Get block count of the node by performing `perform_request('getblockcount')
        """
        return self.perform_request("getblockcount")

    def stop(self):
        """
        Perform the `stop` RPC command to utreexod and some cleanup on process and files
        """
        result = self.perform_request("stop")
        self.wait_for_connections(opened=False)
        return result

    def get_peerinfo(self) -> dict:
        """
        Get the peer information by performing `perform_request('getpeerinfo')`
        """
        return self.perform_request("getpeerinfo")

    def ping(self) -> None:
        """
        Perform the `ping` RPC that checks if our peers are alive.
        """
        self.perform_request("ping")

    def get_rpcinfo(self):
        """
        Returns stats about our RPC server performing
        `perform_request('getrpcinfo')`
        """
        return self.perform_request("getrpcinfo")

    def uptime(self) -> int:
        """
        Get the uptime of the node by performing `perform_request('uptime')`
        """
        return self.perform_request("uptime")

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

    def get_txout(self, txid: str, vout: int, include_mempool: bool) -> dict:
        """
        Get transaction output by performing `perform_request('gettxout', params=[str, int])`

        Args:
            txid: The transaction ID
            vout: The output index

        Returns:
            The transaction output information
        """
        return self.perform_request("gettxout", params=[txid, vout, include_mempool])

    def generate_block_to_address(self, nblocks: int, address: str) -> list:
        """
        Mine blocks immediately to a specific address by using
        `perform_request('generatetoaddress', params=[int, str])`

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
