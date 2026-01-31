"""
Test the wallet configuration using configuration files for the Floresta node.
"""

import os

from test_framework import FlorestaTestFramework
from test_framework.node import Node, NodeType
from test_framework.constants import (
    WALLET_ADDRESS,
    WALLET_DESCRIPTOR_EXTERNAL,
    WALLET_DESCRIPTOR_INTERNAL,
    WALLET_DESCRIPTOR_PRIV_EXTERNAL,
    WALLET_DESCRIPTOR_PRIV_INTERNAL,
    WALLET_XPUB_BIP_84,
    WALLET_XPRIV,
)

WALLET_CONFIG_ADDRESS = "\n".join(
    [
        "[wallet]",
        f'addresses = [ "{WALLET_ADDRESS}" ]',
    ]
)
WALLET_CONFIG_XPUB = "\n".join(
    [
        "[wallet]",
        f'xpubs = [ "{WALLET_XPUB_BIP_84}" ]',
    ]
)

WALLET_CONFIG_DESCRIPTOR = "\n".join(
    [
        "[wallet]",
        f'descriptors = [ "{WALLET_DESCRIPTOR_EXTERNAL}", "{WALLET_DESCRIPTOR_INTERNAL}" ]',
    ]
)

WALLET_CONFIG_XPRIV = "\n".join(
    [
        "[wallet]",
        f'xpubs = [ "{WALLET_XPRIV}" ]',
    ]
)

WALLET_CONFIG_DESCRIPTOR_PRIV = "\n".join(
    [
        "[wallet]",
        f'descriptors = [ "{WALLET_DESCRIPTOR_PRIV_EXTERNAL}", "{WALLET_DESCRIPTOR_PRIV_INTERNAL}" ]',
    ]
)


class WalletConfTest(FlorestaTestFramework):
    """
    Test the wallet configuration using configuration files for the Floresta node.

    This class tests the behavior of different wallet configurations, ensuring
    that the node handles them correctly.
    """

    def set_test_params(self):
        """
        Set up five nodes with different wallet configurations.
        """
        self.florestad_addr = self.create_floresta_node(WALLET_CONFIG_ADDRESS)
        self.florestad_xpub = self.create_floresta_node(WALLET_CONFIG_XPUB)
        self.florestad_desc = self.create_floresta_node(WALLET_CONFIG_DESCRIPTOR)
        self.florestad_xpriv = self.create_floresta_node(WALLET_CONFIG_XPRIV)
        self.florestad_desc_priv = self.create_floresta_node(
            WALLET_CONFIG_DESCRIPTOR_PRIV
        )

    def create_floresta_node(self, config):
        """
        Create Floresta nodes with the given configuration.
        """
        floresta_node = self.add_node_default_args(variant=NodeType.FLORESTAD)
        config_dir = os.path.join(floresta_node.daemon.data_dir, "config.toml")
        with open(config_dir, "w") as f:
            f.write(config)
            floresta_node.set_extra_args([f"--config-file={config_dir}"])

        return floresta_node

    def run_test(self):
        """
        Run the test cases for each node configuration.
        """
        self.run_node(self.florestad_addr)
        self.run_node(self.florestad_xpub)
        self.run_node(self.florestad_desc)

        self.log("Checking descriptors for each wallet(addr)")
        descriptors = self.florestad_addr.rpc.list_descriptors()
        self.assertEqual(len(descriptors), 0)

        self.log("Checking descriptors for each wallet(xpub)")
        descriptors = self.florestad_xpub.rpc.list_descriptors()
        self.assertEqual(len(descriptors), 2)
        self.assertEqual(descriptors[0], WALLET_DESCRIPTOR_EXTERNAL)
        self.assertEqual(descriptors[1], WALLET_DESCRIPTOR_INTERNAL)

        self.log("Checking descriptors for each wallet(descriptor)")
        descriptors = self.florestad_desc.rpc.list_descriptors()
        self.assertEqual(len(descriptors), 2)
        self.assertEqual(descriptors[0], WALLET_DESCRIPTOR_EXTERNAL)
        self.assertEqual(descriptors[1], WALLET_DESCRIPTOR_INTERNAL)

        self.log("Checking descriptors for each wallet(xpriv)")
        with self.assertRaises(Exception):
            self.run_node(self.florestad_xpriv)

        self.log("Checking descriptors for each wallet(descriptor with privkey)")
        with self.assertRaises(Exception):
            self.run_node(self.florestad_desc_priv)


if __name__ == "__main__":
    WalletConfTest().main()
