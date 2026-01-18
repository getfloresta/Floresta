"""
Test the --connect cli option of florestad

This test will start a utreexod, then start a florestad node with
the --connect option pointing to the utreexod node. Then check if
the utreexod node is connected to the florestad node.
"""

from test_framework import FlorestaTestFramework, NodeType
import time

SLEEP_TIME = 10


class CliConnectTest(FlorestaTestFramework):

    def set_test_params(self):
        self.utreexod = self.add_node_default_args(variant=NodeType.UTREEXOD)

        # To get the random port we nee to start the utreexod first
        self.log("=== Starting utreexod")
        self.run_node(self.utreexod)

        # Now we can start the florestad node with the connect option
        to_connect = self.utreexod.p2p_url
        self.florestad = self.add_node_extra_args(
            variant=NodeType.FLORESTAD,
            extra_args=[f"--connect={to_connect}"],
        )

    def run_test(self):
        # Start the nodes
        self.log("=== Starting floresta")
        self.run_node(self.florestad)

        time.sleep(SLEEP_TIME)  # Give some time for the nodes to start

        # Check whether the utreexod is connected to florestad
        self.log("=== Checking connection")
        res = self.utreexod.rpc.get_peerinfo()
        self.assertEqual(len(res), 1)


if __name__ == "__main__":
    CliConnectTest().main()
