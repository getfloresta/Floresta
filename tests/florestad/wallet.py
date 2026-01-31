"""
Test the wallet configuration using configuration files for the Floresta node.
"""

import os
import pytest

from test_framework.node import NodeType
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
        f'descriptors = [ "{WALLET_DESCRIPTOR_PRIV_EXTERNAL}", '
        f'"{WALLET_DESCRIPTOR_PRIV_INTERNAL}" ]',
    ]
)

EXPECTED_DESCRIPTORS = [
    WALLET_DESCRIPTOR_EXTERNAL,
    WALLET_DESCRIPTOR_INTERNAL,
]


@pytest.mark.florestad
def test_wallet_conf(setup_logging, node_manager):
    """
    Test the wallet configuration using configuration files for the Floresta node.
    """
    log = setup_logging
    test = WalletConfigTest(log)

    log.info("Testing wallet configuration with xpub using config files")
    test.test_valid_config(
        lambda: create_floresta_node(node_manager, WALLET_CONFIG_XPUB)
    )

    log.info("Testing wallet configuration with descriptor")
    test.test_valid_config(
        lambda: create_floresta_node(node_manager, WALLET_CONFIG_DESCRIPTOR)
    )

    log.info("Testing wallet configuration with address (no descriptors)")
    test.test_empty_descriptors(
        lambda: create_floresta_node(node_manager, WALLET_CONFIG_ADDRESS)
    )

    log.info("Testing wallet configuration with xpriv (invalid)")
    test.test_invalid_config(
        lambda: create_floresta_node(node_manager, WALLET_CONFIG_XPRIV)
    )

    log.info("Testing wallet configuration with private descriptor (invalid)")
    test.test_invalid_config(
        lambda: create_floresta_node(node_manager, WALLET_CONFIG_DESCRIPTOR_PRIV)
    )


@pytest.mark.florestad
def test_wallet_flags(setup_logging, add_node_with_extra_args):
    """
    Test the wallet configuration flags for the Floresta node.
    """
    log = setup_logging
    test = WalletConfigTest(log)

    log.info("Testing wallet flags with descriptors")
    test.test_valid_config(
        lambda: add_node_with_extra_args(
            variant=NodeType.FLORESTAD,
            extra_args=[
                f"--wallet-descriptor={WALLET_DESCRIPTOR_EXTERNAL}",
                f"--wallet-descriptor={WALLET_DESCRIPTOR_INTERNAL}",
            ],
        )
    )

    log.info("Testing wallet flags with xpub")
    test.test_valid_config(
        lambda: add_node_with_extra_args(
            variant=NodeType.FLORESTAD,
            extra_args=[f"--wallet-xpub={WALLET_XPUB_BIP_84}"],
        )
    )

    log.info("Testing wallet flags with xpriv (invalid)")
    test.test_invalid_config(
        lambda: add_node_with_extra_args(
            variant=NodeType.FLORESTAD,
            extra_args=[f"--wallet-xpub={WALLET_XPRIV}"],
        )
    )

    log.info("Checking Wallet descriptor with privkey")
    test.test_invalid_config(
        lambda: add_node_with_extra_args(
            variant=NodeType.FLORESTAD,
            extra_args=[
                f"--wallet-descriptor={WALLET_DESCRIPTOR_PRIV_EXTERNAL}",
                f"--wallet-descriptor={WALLET_DESCRIPTOR_PRIV_INTERNAL}",
            ],
        )
    )


def create_floresta_node(node_manager, config):
    """
    Create Floresta nodes with the given configuration.
    """
    floresta_node = node_manager.add_node_default_args(variant=NodeType.FLORESTAD)
    config_dir = os.path.join(floresta_node.daemon.data_dir, "config.toml")
    with open(config_dir, "w", encoding="utf-8") as f:
        f.write(config)
        floresta_node.set_extra_args([f"--config-file={config_dir}"])

    node_manager.run_node(floresta_node)

    return floresta_node


class WalletConfigTest:
    """
    Centralized wallet configuration test validator.
    Tests node creation and descriptor validation with support for both
    valid and invalid configurations.
    """

    def __init__(self, log):
        self.log = log

    def test_valid_config(self, node_creator):
        """
        Test valid wallet configuration: creates node, validates descriptors,
        and ensures no duplication on restart.

        Args:
            node_creator: Callable that creates and returns a configured node
        """
        node = node_creator()
        self.validate_wallet_configuration(node)

    def test_invalid_config(self, node_creator):
        """
        Test invalid wallet configuration: expects exception during node creation.

        Args:
            node_creator: Callable that attempts to create a node with invalid config
        """
        with pytest.raises(Exception):
            node_creator()

    def test_empty_descriptors(self, node_creator):
        """
        Test configuration that results in no descriptors.

        Args:
            node_creator: Callable that creates a node with no descriptors
        """
        node = node_creator()
        descriptors = node.rpc.list_descriptors()

        assert len(descriptors) == 0

    def validate_wallet_configuration(self, node):
        """
        Validate the wallet configuration by checking the node descriptors before and after
        restarting the node, ensuring they are not duplicated on restart.
        """
        self.check_descriptors(node)

        self.log.debug("Restarting node to check for descriptor duplication")
        node.stop()
        node.start()

        self.check_descriptors(node)

    def check_descriptors(self, node):
        """
        Check the node descriptors against the expected descriptors.
        """
        descriptors = node.rpc.list_descriptors()
        assert len(descriptors) == len(EXPECTED_DESCRIPTORS)

        for descriptor, expected in zip(descriptors, EXPECTED_DESCRIPTORS):
            assert descriptor == expected
