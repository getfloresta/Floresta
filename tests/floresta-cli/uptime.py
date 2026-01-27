"""
floresta_cli_uptime.py

This functional test cli utility to interact with a Floresta node with `uptime`
"""

import time
from test_framework import FlorestaTestFramework, Node, NodeType


class UptimeTest(FlorestaTestFramework):
    """
    Test `uptime` rpc call, by creating a node, wait for
    some time (10 seconds) and assert that this wait time is
    equal to how long florestad has been running
    """

    def set_test_params(self):
        """
        Setup the two node florestad process with different data-dirs, electrum-addresses
        and rpc-addresses in the same regtest network
        """
        self.florestad = self.add_node_default_args(
            variant=NodeType.FLORESTAD,
        )

        self.bitcoind = self.add_node_default_args(
            variant=NodeType.BITCOIND,
        )

    def test_node_uptime(self, node: Node, test_time: int, margin: int):
        """
        Test the uptime of a node, given an index
        by checking if the uptime matches the elapsed
        time after starting the node with a grace period
        for startup and function call times
        """
        self.run_node(node)
        before = time.time()
        time.sleep(test_time)
        result = node.rpc.uptime()
        after = time.time()
        elapsed = int(after - before)

        self.assertTrue(result >= elapsed and result <= elapsed + margin)
        return result

    def run_test(self):
        """
        Run JSONRPC server on first, wait to connect, then call `addnode ip[:port]`
        """
        self.test_node_uptime(node=self.florestad, test_time=15, margin=15)
        self.test_node_uptime(node=self.bitcoind, test_time=15, margin=15)


if __name__ == "__main__":
    UptimeTest().main()
