"""
floresta_cli_getroots.py

This functional test cli utility to interact with a Floresta node with `getroots`
"""

from test_framework import FlorestaTestFramework, NodeType


class GetRootsIDBLenZeroTest(FlorestaTestFramework):
    """
    Test `getroots` rpc call,
    """

    def set_test_params(self):
        """
        Setup the two node florestad process with different data-dirs, electrum-addresses
        and rpc-addresses in the same regtest network
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

    def run_test(self):
        """
        Run JSONRPC server on first, wait to connect, then call `addnode ip[:port]`
        """
        # Start node
        self.run_node(self.florestad)

        # Test assertions
        vec_hashes = self.florestad.rpc.get_roots()
        self.assertTrue(len(vec_hashes) == 0)


if __name__ == "__main__":
    GetRootsIDBLenZeroTest().main()
