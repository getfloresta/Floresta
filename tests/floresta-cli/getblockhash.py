"""
getblockhash.py

This functional test cli utility to interact with a Floresta node with `getblockhash`
"""

import re
import time
from test_framework import FlorestaTestFramework, NodeType


class GetBlockhashTest(FlorestaTestFramework):
    """
    Test `getblockhash` with a fresh node and expected initial 0 height block.
    After that, it will mine some blocks with utreexod and check that
    the blockhashes match between floresta, utreexod, and bitcoind.
    """

    best_block = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"

    def set_test_params(self):
        """
        Setup a single node
        """
        name = self.__class__.__name__.lower()
        self.v2transport = False

        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

        self.utreexod = self.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                "--miningaddr=bcrt1q4gfcga7jfjmm02zpvrh4ttc5k7lmnq2re52z2y",
                "--prune=0",
            ],
        )

        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def run_test(self):
        """
        Run JSONRPC and get the hash of heights 0 to 10.
        """
        self.log("=== Starting nodes...")
        self.run_node(self.florestad)
        self.run_node(self.utreexod)
        self.run_node(self.bitcoind)

        self.log("=== Mining blocks with utreexod")
        self.utreexod.rpc.generate(10)
        time.sleep(5)

        self.log("=== Connect floresta to utreexod")
        utreexod_url = self.utreexod.p2p_url
        self.florestad.rpc.addnode(utreexod_url, command="onetry", v2transport=False)

        self.log("=== Waiting for floresta to connect to utreexod...")
        time.sleep(5)
        peer_info = self.florestad.rpc.get_peerinfo()
        self.assertMatch(
            peer_info[0]["user_agent"],
            re.compile(r"/btcwire:\d+\.\d+\.\d+/utreexod:\d+\.\d+\.\d+/"),
        )

        self.log("=== Connect bitcoind to utreexod")
        self.bitcoind.rpc.addnode(utreexod_url, command="onetry", v2transport=False)

        self.log("=== Waiting for bitcoind to connect to utreexod...")
        time.sleep(5)
        peer_info = self.bitcoind.rpc.get_peerinfo()
        self.assertMatch(
            peer_info[0]["subver"],
            re.compile(r"/btcwire:\d+\.\d+\.\d+/utreexod:\d+\.\d+\.\d+/"),
        )

        self.log("=== Wait for the nodes to sync...")
        time.sleep(5)

        self.log("=== Get the tip block")
        block_count = self.florestad.rpc.get_block_count()

        for i in range(0, block_count + 1):
            self.log(f"=== Check the correct blockhash for height {i}...")
            hash_floresta = self.florestad.rpc.get_blockhash(i)
            hash_utreexod = self.utreexod.rpc.get_blockhash(i)
            hash_bitcoind = self.bitcoind.rpc.get_blockhash(i)
            for _hash in [hash_utreexod, hash_bitcoind]:
                self.assertEqual(hash_floresta, _hash)


if __name__ == "__main__":
    GetBlockhashTest().main()
