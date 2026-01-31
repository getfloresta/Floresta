"""
Test the wallet configuration flags for the Floresta node.

This script tests the behavior of the `--wallet-xpub` and `--wallet-descriptor`
flags, ensuring that the node handles them correctly.
"""

import os

from test_framework import FlorestaTestFramework
from test_framework.constants import (
    WALLET_ADDRESS,
    WALLET_DESCRIPTOR_EXTERNAL,
    WALLET_DESCRIPTOR_INTERNAL,
    WALLET_DESCRIPTOR_PRIV_EXTERNAL,
    WALLET_DESCRIPTOR_PRIV_INTERNAL,
    WALLET_XPUB_BIP_84,
    WALLET_XPRIV,
)

DATA_DIR = FlorestaTestFramework.get_integration_test_dir()


class WalletFlagTest(FlorestaTestFramework):
    """
    Test the wallet configuration flags for the Floresta node.
    """

    def set_test_params(self):
        """
        Set up four nodes with different wallet configurations.
        """
        name = self.__class__.__name__.lower()

        self.data_dirs = self.create_data_dirs(DATA_DIR, name, 4)

        for index, data_dir in enumerate(self.data_dirs):
            if index == 0:
                self.florestad_xpub = self.add_node(
                    variant="florestad",
                    extra_args=[
                        f"--wallet-xpub={WALLET_XPUB_BIP_84}",
                        f"--data-dir={data_dir}",
                    ],
                )

            elif index == 1:
                self.florestad_desc = self.add_node(
                    variant="florestad",
                    extra_args=[
                        f"--wallet-descriptor={WALLET_DESCRIPTOR_EXTERNAL}",
                        f"--wallet-descriptor={WALLET_DESCRIPTOR_INTERNAL}",
                        f"--data-dir={data_dir}",
                    ],
                )

            elif index == 2:
                self.florestad_xpriv = self.add_node(
                    variant="florestad",
                    extra_args=[
                        f"--wallet-xpub={WALLET_XPRIV}",
                        f"--data-dir={data_dir}",
                    ],
                )

            elif index == 3:
                self.florestad_desc_priv = self.add_node(
                    variant="florestad",
                    extra_args=[
                        f"--wallet-descriptor={WALLET_DESCRIPTOR_PRIV_EXTERNAL}",
                        f"--wallet-descriptor={WALLET_DESCRIPTOR_PRIV_INTERNAL}",
                        f"--data-dir={data_dir}",
                    ],
                )

            else:
                break

    def run_test(self):
        """
        Run the test cases for each node configuration.
        """
        self.run_node(self.florestad_xpub)
        self.run_node(self.florestad_desc)

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
    WalletFlagTest().main()
