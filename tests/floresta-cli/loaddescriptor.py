"""
floresta_cli_loaddescriptor.py

Functional test for the `loaddescriptor` CLI utility to interact with a Floresta node.
"""

import pytest
from requests.exceptions import HTTPError

from test_framework.constants import (
    WALLET_DESCRIPTOR_EXTERNAL,
    WALLET_DESCRIPTOR_INTERNAL,
    WALLET_DESCRIPTOR_PRIV_EXTERNAL,
)


@pytest.mark.rpc
def test_load_descriptor(setup_logging, florestad_node):
    """
    Test the `loaddescriptor` RPC command with a fresh node.
    """
    log = setup_logging

    log.info("Loading external wallet descriptor...")
    result = florestad_node.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)
    assert result

    check_descriptors(florestad_node, log)

    log.info("Loading private key wallet descriptor (should fail)...")
    with pytest.raises(HTTPError):
        result = florestad_node.rpc.load_descriptor(WALLET_DESCRIPTOR_PRIV_EXTERNAL)

    check_descriptors(florestad_node, log)

    log.info("Loading external wallet descriptor again (should fail)...")
    with pytest.raises(HTTPError):
        result = florestad_node.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)

    check_descriptors(florestad_node, log)

    log.info("Loading internal wallet descriptor...")
    result = florestad_node.rpc.load_descriptor(WALLET_DESCRIPTOR_INTERNAL)
    assert result

    descriptors = florestad_node.rpc.list_descriptors()
    assert len(descriptors) == 2
    assert descriptors[0] == WALLET_DESCRIPTOR_EXTERNAL
    assert descriptors[1] == WALLET_DESCRIPTOR_INTERNAL


def check_descriptors(florestad, log):
    """
    Check the descriptors loaded in the node.
    """
    log.debug("Checking loaded descriptors...")
    descriptors = florestad.rpc.list_descriptors()

    assert len(descriptors) == 1
    assert descriptors[0] == WALLET_DESCRIPTOR_EXTERNAL
