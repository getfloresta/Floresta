"""
floresta_cli_listdescriptor.py

This functional test cli utility to interact with a Floresta node with `listdescriptor`
"""

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.constants import WALLET_DESCRIPTOR_EXTERNAL


class ListDescriptorTest(FlorestaTestFramework):
    """
    Test the `listdescriptors` RPC command with a fresh node.
    """

    def set_test_params(self):
        """
        Setup a single node
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

    def run_test(self):
        self.run_node(self.florestad)

        self.log("Checking initial descriptors...")
        descriptors = self.florestad.rpc.list_descriptors()
        self.assertEqual(len(descriptors), 0)

        self.log("Loading external wallet descriptor...")
        result = self.florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)
        self.assertTrue(result)

        self.log("Checking loaded descriptors...")
        descriptors = self.florestad.rpc.list_descriptors()
        self.assertEqual(len(descriptors), 1)
        self.assertEqual(descriptors[0], WALLET_DESCRIPTOR_EXTERNAL)


if __name__ == "__main__":
    ListDescriptorTest().main()
