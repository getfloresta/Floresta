"""
floresta_cli_getrpcinfo.py

This functional test cli utility to interact with a Floresta node with `getrpcinfo`
"""

import os
from test_framework import FlorestaTestFramework

DATA_DIR = FlorestaTestFramework.get_integration_test_dir()


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
        # Create data directories for the nodes
        self.data_dirs = GetRpcInfoTest.create_data_dirs(
            DATA_DIR, self.__class__.__name__.lower(), nodes=2
        )

        # Now create the nodes with the data directories
        self.florestad = self.add_node(
            variant="florestad",
            extra_args=[
                f"--data-dir={self.data_dirs[0]}",
            ],
        )

        self.bitcoind = self.add_node(
            variant="bitcoind",
            extra_args=[
                f"-datadir={self.data_dirs[1]}",
            ],
        )

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
        # Check the logpath. Paths may differ (e.g., relative vs absolute),
        # so verify it contains the expected information.
        self.assertTrue(os.path.normpath(expected_logpath) in result["logpath"])

    def test_floresta_getrpcinfo(self):
        """
        Test the `getrpcinfo` rpc call by creating a node
        and checking the response in florestad.
        """
        result = self.florestad.rpc.get_rpcinfo()
        expected_logpath = os.path.join(self.data_dirs[0], "regtest", "debug.log")
        self.assert_rpcinfo_structure(result, expected_logpath)

    def test_bitcoind_getrpcinfo(self):
        """
        Test the `getrpcinfo` rpc call by creating a node
        and checking the response in bitcoind.
        """
        result = self.bitcoind.rpc.get_rpcinfo()
        expected_logpath = os.path.join(self.data_dirs[1], "regtest", "debug.log")
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
