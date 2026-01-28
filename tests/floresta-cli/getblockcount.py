"""
Test the floresta's `getblockcount` before and after mining a few blocks with
utreexod. Then, assert that the command returns the same number of
`blocks` and `height/validated` fields given in `getblockchaininfo`
of utreexod/bitcoind and floresta, respectively"""

import re
import time
from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.constants import WALLET_ADDRESS


class GetBlockCountTest(FlorestaTestFramework):
    """
    Test florestad's `getbestblockhash` by running three nodes in
    a "semi-triangle" network structure, where florestad and bitcoind
    nodes are connected to utreexod, but not connected between them.
    Then assert that the same blockcount in three nodes, before mining
    and after mining.
    """

    def set_test_params(self):
        """
        Setup a florestad/bitcoind peers and a utreexod mining node
        """
        name = self.__class__.__name__.lower()
        self.v2transport = False
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

        self.utreexod = self.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                f"--miningaddr={WALLET_ADDRESS}",
                "--prune=0",
            ],
        )

        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def run_test(self):
        """
        Run a florestad/bitcoind/utreexod nodes. Then mine some blocks
        with utreexod. After that, connect the nodes and wait for them to sync.
        Finally, test the `getblockcount` rpc command checking if it's
        different from genesis one and equals to utreexod one.
        """
        self.run_node(self.florestad)
        self.run_node(self.utreexod)
        self.run_node(self.bitcoind)

        self.log("=== Get genesis block count...")
        chain_floresta = self.florestad.rpc.get_blockchain_info()
        chain_utreexod = self.utreexod.rpc.get_blockchain_info()
        chain_bitcoind = self.bitcoind.rpc.get_blockchain_info()
        height_floresta = self.florestad.rpc.get_block_count()
        height_utreexod = self.utreexod.rpc.get_block_count()
        height_bitcoind = self.bitcoind.rpc.get_block_count()

        for height in [
            0,
            chain_floresta["height"],
            chain_utreexod["blocks"],
            chain_bitcoind["blocks"],
            height_utreexod,
            height_bitcoind,
        ]:
            self.assertEqual(height_floresta, height)

        self.log("=== Mining blocks with utreexod")
        self.utreexod.rpc.generate(10)
        time.sleep(5)

        self.log("=== Connect floresta to utreexod")
        self.connect_nodes(self.florestad, self.utreexod)

        self.log("=== Connect bitcoind to utreexod")
        self.connect_nodes(self.bitcoind, self.utreexod)

        self.log("=== Wait for the nodes to sync...")
        time.sleep(20)

        self.log("=== Check that floresta has the same blockcount as utreexod...")
        floresta_chain = self.florestad.rpc.get_blockchain_info()
        utreexod_chain = self.utreexod.rpc.get_blockchain_info()
        height_florestad = self.florestad.rpc.get_block_count()
        height_utreexod = self.utreexod.rpc.get_block_count()

        self.assertEqual(height_florestad, height_utreexod)
        self.assertEqual(height_florestad, floresta_chain["validated"])
        self.assertEqual(height_florestad, floresta_chain["height"])
        self.assertEqual(height_florestad, utreexod_chain["blocks"])

        self.log("=== Check that florestad has the same blockcount as bitcoind...")
        bitcoind_chain = self.bitcoind.rpc.get_blockchain_info()
        height_bitcoind = self.bitcoind.rpc.get_block_count()

        self.assertEqual(height_florestad, height_bitcoind)
        self.assertEqual(height_florestad, bitcoind_chain["blocks"])


if __name__ == "__main__":
    GetBlockCountTest().main()
