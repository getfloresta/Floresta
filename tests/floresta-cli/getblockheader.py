"""
getblockheader.py

This functional test cli utility to interact with a Floresta node with `getblockheader`
compliant with Bitcoin-Core.

It starts three nodes, one miner (utreexod) and two sync nodes (florestad and bitcoind).
The miner is also a bridge node.

The miner will mine 10 blocks, and the sync nodes will update their states. Once updated,
the sync nodes  will call for `getblockheader` rpc command.
"""

import re
import time
from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


BLOCKS = 10


class GetBlockheaderTest(FlorestaTestFramework):
    """
    Test florestad's `getblockheader` by running three nodes in
    a "semi-triangle" network structure, where florestad and bitcoind
    nodes are connected to utreexod, but not connected between them.
    Then assert that the same get_blockheader between florestad and core
    are equal.
    """

    def set_test_params(self):
        """
        Setup a florestad/bitcoind peers and a utreexod mining node
        """
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
        Run a florestad/bitcoind/utreexod nodes. Then mine some blocks
        with utreexod. After that, connect the nodes and wait for them to sync.
        Finally, test the `getblockheader` rpc command checking if it's
        different from genesis one and equals to utreexod one.
        """
        self.run_node(self.florestad)
        self.run_node(self.utreexod)
        self.run_node(self.bitcoind)

        self.log("=== Mining  with utreexod")
        self.utreexod.rpc.generate(BLOCKS)
        time.sleep(5)

        self.log("=== Connect floresta to utreexod")
        self.connect_nodes(self.florestad, self.utreexod)
        peer_info = self.florestad.rpc.get_peerinfo()
        self.assertMatch(
            peer_info[0]["user_agent"],
            re.compile(r"/btcwire:\d+\.\d+\.\d+/utreexod:\d+\.\d+\.\d+/"),
        )

        self.log("=== Connect bitcoind to utreexod")
        self.connect_nodes(self.bitcoind, self.utreexod)
        peer_info = self.bitcoind.rpc.get_peerinfo()
        self.assertMatch(
            peer_info[0]["subver"],
            re.compile(r"/btcwire:\d+\.\d+\.\d+/utreexod:\d+\.\d+\.\d+/"),
        )

        for height in range(BLOCKS):
            self.log(
                f"=== Check floresta have the same blockheader as core for height {height}..."
            )
            floresta_hash = self.florestad.rpc.get_blockhash(height)
            bitcoin_hash = self.bitcoind.rpc.get_blockhash(height)
            self.assertEqual(floresta_hash, bitcoin_hash)

            floresta_blk = self.florestad.rpc.get_blockheader(floresta_hash, False)
            bitcoin_blk = self.bitcoind.rpc.get_blockheader(bitcoin_hash, False)
            self.assertEqual(floresta_blk, bitcoin_blk)

            floresta_verbose_blk = self.florestad.rpc.get_blockheader(
                floresta_hash, True
            )
            bitcoin_verbose_blk = self.bitcoind.rpc.get_blockheader(bitcoin_hash, True)

            # Not test in regtest network the "difficulty" field since
            # rust-bitcoin apply correctly while core have a bug in this field
            for key in (
                "hash",
                "confirmations",
                "height",
                "version",
                "versionHex",
                "merkleroot",
                "time",
                "mediantime",
                "nonce",
                "bits",
                "target",
                "chainwork",
                "nTx",
            ):
                self.assertEqual(floresta_verbose_blk[key], bitcoin_verbose_blk[key])

            # These assertions will run only in some
            # general conditions like the height
            if height != 0:
                self.assertEqual(
                    floresta_verbose_blk["previousblockhash"],
                    bitcoin_verbose_blk["previousblockhash"],
                )

                if height > 1:
                    self.assertAlmostEqual(
                        floresta_verbose_blk["difficulty"],
                        bitcoin_verbose_blk["difficulty"],
                    )

            self.assertEqual(
                floresta_verbose_blk["nextblockhash"],
                bitcoin_verbose_blk["nextblockhash"],
            )


if __name__ == "__main__":
    GetBlockheaderTest().main()
