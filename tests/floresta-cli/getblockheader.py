# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getblockheader.py

This functional test cli utility to interact with a Floresta node with `getblockheader`
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class GetBlockheaderHeightZeroTest(FlorestaTestFramework):
    """
    Test `getblockheader` with a fresh node and expect a verbose result
    matching Bitcoin Core's response format for the regtest genesis block.
    """

    nodes = [-1]
    blockhash = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"

    def set_test_params(self):
        """
        Setup a single node
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

    def run_test(self):
        """
        Run JSONRPC and get the header of the genesis block
        """
        # Start node
        self.run_node(self.florestad)

        # Test assertions
        response = self.florestad.rpc.get_blockheader(
            GetBlockheaderHeightZeroTest.blockhash
        )

        self.assertEqual(
            response["hash"],
            "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206",
        )
        self.assertEqual(response["confirmations"], 1)
        self.assertEqual(response["height"], 0)
        self.assertEqual(response["version"], 1)
        self.assertEqual(response["versionHex"], "00000001")
        self.assertEqual(
            response["merkleroot"],
            "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
        )
        self.assertEqual(response["time"], 1296688602)
        self.assertEqual(response["mediantime"], 1296688602)
        self.assertEqual(response["nonce"], 2)
        self.assertEqual(response["bits"], "207fffff")
        self.assertEqual(
            response["target"],
            "7fffff0000000000000000000000000000000000000000000000000000000000",
        )

        # Genesis block should not have previousblockhash
        self.assertEqual("previousblockhash" in response, False)

        # Verify additional fields are present
        self.assertIn("difficulty", list(response.keys()))
        self.assertIn("chainwork", list(response.keys()))
        self.assertIn("nTx", list(response.keys()))


if __name__ == "__main__":
    GetBlockheaderHeightZeroTest().main()
