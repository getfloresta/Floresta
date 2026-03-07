"""
floresta_cli_getblockheader.py

This functional test cli utility to interact with a Floresta node with `getblockheader`
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.constants import GENESIS_BLOCK_HASH


class GetBlockheaderHeightZeroTest(FlorestaTestFramework):
    """
    Test `getblockheader` with a fresh node and expect a result like this:

    ````bash
    $> ./target/release floresta_cli --network=regtest getblockheader \
        0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206
    {
       "version": 1,
       "prev_blockhash": "0000000000000000000000000000000000000000000000000000000000000000",
       "merkle_root": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
       "time": 1296688602,
       "bits": 545259519,
       "nonce": 2
    }
    ```
    """

    nodes = [-1]
    version = 1
    prev_blockhash = "0000000000000000000000000000000000000000000000000000000000000000"
    merkle_root = "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b"
    time = 1296688602
    bits = 545259519
    nonce = 2

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
        response = self.florestad.rpc.get_blockheader(GENESIS_BLOCK_HASH)
        self.assertEqual(response["version"], GetBlockheaderHeightZeroTest.version)
        self.assertEqual(
            response["prev_blockhash"], GetBlockheaderHeightZeroTest.prev_blockhash
        )
        self.assertEqual(
            response["merkle_root"], GetBlockheaderHeightZeroTest.merkle_root
        )
        self.assertEqual(response["time"], GetBlockheaderHeightZeroTest.time)
        self.assertEqual(response["bits"], GetBlockheaderHeightZeroTest.bits)
        self.assertEqual(response["nonce"], GetBlockheaderHeightZeroTest.nonce)


if __name__ == "__main__":
    GetBlockheaderHeightZeroTest().main()
