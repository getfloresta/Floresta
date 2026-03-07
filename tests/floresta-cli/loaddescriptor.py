"""
floresta_cli_loaddescriptor.py

Functional test for the `loaddescriptor` CLI utility to interact with a Floresta node.
"""

from requests.exceptions import HTTPError

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType
from test_framework.constants import (
    WALLET_DESCRIPTOR_EXTERNAL,
    WALLET_DESCRIPTOR_PRIV_EXTERNAL,
)


class LoadDescriptorTest(FlorestaTestFramework):
    """
    Test the `loaddescriptor` RPC command with a fresh node.
    """

    def set_test_params(self):
        """
        Setup a single node
        """
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

    def check_descriptors(self):
        """
        Check the descriptors loaded in the node.
        """
        self.log("Checking loaded descriptors...")
        descriptors = self.florestad.rpc.list_descriptors()
        self.assertEqual(len(descriptors), 1)
        self.assertEqual(descriptors[0], WALLET_DESCRIPTOR_EXTERNAL)

    def run_test(self):
        self.run_node(self.florestad)

        self.log("Loading external wallet descriptor...")
        result = self.florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)
        self.assertTrue(result)

        self.check_descriptors()

        self.log("Loading private key wallet descriptor (should fail)...")
        with self.assertRaises(HTTPError):
            result = self.florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_PRIV_EXTERNAL)

        self.check_descriptors()

        self.log("Loading external wallet descriptor again (should fail)...")
        with self.assertRaises(HTTPError):
            result = self.florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)

        self.check_descriptors()


if __name__ == "__main__":
    LoadDescriptorTest().main()
