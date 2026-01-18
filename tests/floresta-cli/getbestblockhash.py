"""
Test the floresta's `getbestblockhash` after mining a few block with
utreexod. Then, assert that the command returns the same hash of
`best_block` or `bestblockhash` given in `getblockchaininfo` of floresta
and utreexod, respectively.
"""

import re
import time
from test_framework import FlorestaTestFramework, NodeType


class GetBestblockhashTest(FlorestaTestFramework):
    """
    Test florestad's `getbestblockhash` by running two nodes, the first
    the florestad itself and utreexod as miner nodes:
    (1) Get the genesis block with `getbestblockhash`;
    (2) mine some blocks with utreexod;
    (3) connect florestad to utreexod;
    (4) wait for the nodes to sync;
    (5) check that `getbestblockhash` returns the same hash as
        `best_block` or `bestblockhash` given in `getblockchaininfo`
        of floresta and utreexod, respectively.
    """

    best_block = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"

    def set_test_params(self):
        """
        Setup a florestad node and a utreexod mining node
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

        self.utreexod = self.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                "--miningaddr=bcrt1q4gfcga7jfjmm02zpvrh4ttc5k7lmnq2re52z2y",
                "--prune=0",
            ],
        )

    def run_test(self):
        """
        Run a florestad node and mine some blocks with utreexod. After that,
        connect floresta to utreexod and wait for the nodes to sync.
        Finally, test the `getbestblockhash` rpc command checking if it's
        different from genesis one and equals to utreexod one.
        """
        self.run_node(self.florestad)
        self.run_node(self.utreexod)

        self.log("=== Get genesis blockhash from floresta...")
        genesis_block = self.florestad.rpc.get_bestblockhash()

        self.log("=== Mining blocks with utreexod")
        self.utreexod.rpc.generate(10)

        self.log("=== Connect floresta to utreexod")
        utreexod_url = self.utreexod.p2p_url
        self.florestad.rpc.addnode(
            node=utreexod_url, command="onetry", v2transport=False
        )

        self.log("=== Waiting for floresta to connect to utreexod...")
        time.sleep(5)
        peer_info = self.florestad.rpc.get_peerinfo()
        self.assertMatch(
            peer_info[0]["user_agent"],
            re.compile(r"/btcwire:\d+\.\d+\.\d+/utreexod:\d+\.\d+\.\d+/"),
        )

        self.log("=== Wait for the nodes to sync...")
        time.sleep(20)

        self.log("=== Check that floresta has the same chain as utreexod...")
        floresta_chain = self.florestad.rpc.get_blockchain_info()
        utreexo_chain = self.utreexod.rpc.get_blockchain_info()
        self.assertEqual(floresta_chain["validated"], 10)
        self.assertEqual(floresta_chain["best_block"], utreexo_chain["bestblockhash"])
        self.assertEqual(floresta_chain["height"], utreexo_chain["blocks"])

        self.log("=== Get tip block in the most-work fully-validated chain")
        floresta_best_block = self.florestad.rpc.get_bestblockhash()
        self.assertNotEqual(floresta_best_block, genesis_block)
        self.assertEqual(floresta_best_block, floresta_chain["best_block"])
        self.assertEqual(floresta_best_block, utreexo_chain["bestblockhash"])


if __name__ == "__main__":
    GetBestblockhashTest().main()
