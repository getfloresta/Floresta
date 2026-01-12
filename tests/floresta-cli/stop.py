"""
floresta_cli_stop.py

This functional test cli utility to interact with a Floresta node with `stop`
"""

import re
from test_framework import FlorestaTestFramework, NodeType


class StopTest(FlorestaTestFramework):
    """
    Test `stop` command with a fresh node and its initial state.
    """

    def set_test_params(self):
        """
        Setup a single node
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)
        self.bitcoind = self.add_node_default_args(variant=NodeType.BITCOIND)

    def run_test(self):
        """
        Run JSONRPC stop command on both flrestad and bitcoin core nodes and
        check if floresta and bitcoin core nodes are stopped correctly and if
        the floresta's stop message is compliant with bitcoin core's stop message.
        """
        self.run_node(self.florestad)
        self.run_node(self.bitcoind)

        # Generally, the self.stop_node() method
        # do all the work for us, but in this case
        # we're testing the method rpc.stop(), so
        # re-do all the steps  to ensure that it
        # was successful and the ports are closed
        result_floresta = self.florestad.rpc.stop()
        result_bitcoin = self.bitcoind.rpc.stop()

        # Check if the messages are correct
        for res in [result_floresta, result_bitcoin]:
            self.assertIsSome(res)
            self.assertIn("stopping", res)
            self.assertMatch(res, re.compile(r"^(Floresta|Bitcoin Core) stopping$"))

        # Check that the node is stopped
        self.florestad.rpc.wait_for_connection(opened=False)
        self.bitcoind.rpc.wait_for_connection(opened=False)


if __name__ == "__main__":
    StopTest().main()
