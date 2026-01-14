"""
tets/test_framework/rpc/utreexo.py

A test framework for testing JsonRPC calls to a utreexo node.
"""

from test_framework.rpc.base import BaseRPC

REGTEST_RPC_SERVER = {
    "host": "127.0.0.1",
    "ports": {
        "p2p": 18444,
        "rpc": 18334,
        "electrum-server": 20001,
        "electrum-server-tls": 20002,
    },
    "user": "utreexo",
    "password": "utreexo",
    "jsonrpc": "1.0",
    "timeout": 10000,
}


class UtreexoRPC(BaseRPC):
    """
    A class for making RPC calls to a utreexo node.
    """

    def get_blockchain_info(self) -> dict:
        """
        Get the blockchain info by performing `perform_request('getblockchaininfo')`
        """
        return self.perform_request("getblockchaininfo")

    def stop(self):
        """
        Perform the `stop` RPC command to utreexod and some cleanup on process and files
        """
        result = self.perform_request("stop")
        self.wait_for_connections(opened=False)
        return result

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

    def send_to_address(self, address: str, amount: float):
        """
        Perform the `sendtoaddress` RPC command to utreexod
        """
        return self.perform_request("sendtoaddress", [address, amount])

    def get_balance(self):
        """
        Perform the `getbalance` RPC command to utreexod
        """
        return self.perform_request("getbalance", [])

    def get_peerinfo(self):
        """
        Perform the `getpeerinfo` RPC command to utreexod
        """
        return self.perform_request("getpeerinfo", [])

    def invalidate_block(self, blockhash: str):
        """
        Invalidate a block by its hash performing
        `perform_request('invalidateblock', params=[<str>])`
        """
        return self.perform_request("invalidateblock", params=[blockhash])

    def get_blockhash(self, height: int) -> str:
        """
        Get the blockhash associated with a given height performing
        `perform_request('getblockhash', params=[<int>])`
        """
        return self.perform_request("getblockhash", [height])

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

    def get_block_count(self) -> int:
        """
        Get block count of the node by performing `perform_request('getblockcount')
        """
        return self.perform_request("getblockcount")

    def get_txout(self, txid: str, vout: int, include_mempool: bool) -> dict:
        """
        Get the outpoint associated with a given tx and vout performing
        `perform_request('gettxout', params=[str, int])`

        Args:
            txid: The transaction id (txid) of the transaction.
            vout: The output index of the transaction.

        Returns:
            A dictionary containing the outpoint information.
        """
        return self.perform_request("gettxout", params=[txid, vout, include_mempool])

    def ping(self) -> None:
        """
        Perform the `ping` RPC that checks if our peers are alive.
        """
        self.perform_request("ping")
