"""
floresta_cli_getrpcinfo.py

This functional test cli utility to interact with a Floresta node with `getrpcinfo`
"""

import os
from test_framework import FlorestaTestFramework, NodeType


class GetRpcInfoTest(FlorestaTestFramework):
    """
    Test `getrpcinfo` rpc call, by creating a node
    """

    nodes = [-1, -1]

    def set_test_params(self):
        """
        Setup the two node florestad process with different data-dirs, electrum-addresses
        and rpc-addresses in the same regtest network
        """
        # Now create the nodes with the data directories
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def assert_rpcinfo_structure(self, result, expected_logpath: str):
        # Ensure only 'active_commands' and 'logpath' are present
        self.assertEqual(set(result.keys()), {"active_commands", "logpath"})
        self.assertEqual(len(result["active_commands"]), 1)

        # Ensure only 'duration' and 'method' are present in command
        command = result["active_commands"][0]
        self.assertEqual(set(command.keys()), {"duration", "method"})

        # Check the command structure
        self.assertEqual(command["method"], "getrpcinfo")
        self.assertTrue(command["duration"] > 0)
        # Check the logpath contains the expected path, but ignore absolute path differences
        self.assertTrue(result["logpath"].endswith(os.path.normpath(expected_logpath)))

    def test_floresta_getrpcinfo(self):
        """
        Test the `getrpcinfo` rpc call by creating a node
        and checking the response in florestad.
        """
        result = self.florestad.rpc.get_rpcinfo()
        expected_logpath = os.path.join(
            self.florestad.daemon.data_dir, "regtest", "debug.log"
        )
        self.assert_rpcinfo_structure(result, expected_logpath)

    def test_bitcoind_getrpcinfo(self):
        """
        Test the `getrpcinfo` rpc call by creating a node
        and checking the response in bitcoind.
        """
        result = self.bitcoind.rpc.get_rpcinfo()
        expected_logpath = os.path.join(
            self.bitcoind.daemon.data_dir, "regtest", "debug.log"
        )
        self.assert_rpcinfo_structure(result, expected_logpath)

    def run_test(self):
        """
        Run JSONRPC server on first, wait to connect, then call `addnode ip[:port]`
        """
        # Start node
        self.run_node(self.florestad)
        self.run_node(self.bitcoind)

        # Test assertions
        self.test_floresta_getrpcinfo()
        self.test_bitcoind_getrpcinfo()


if __name__ == "__main__":
    GetRpcInfoTest().main()
