"""
floresta_cli_listdescriptor.py

This functional test cli utility to interact with a Floresta node with `listdescriptor`
"""

import pytest

from test_framework.constants import WALLET_DESCRIPTOR_EXTERNAL


@pytest.mark.rpc
def test_list_descriptors(setup_logging, florestad_node):
    """
    Test the `listdescriptors` RPC command with a fresh node.
    """
    log = setup_logging

    log.info("Checking initial descriptors...")
    descriptors = florestad_node.rpc.list_descriptors()
    assert len(descriptors) == 0

    log.info("Loading external wallet descriptor...")
    result = florestad_node.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)
    assert result

    log.info("Checking loaded descriptors...")
    descriptors = florestad_node.rpc.list_descriptors()
    assert len(descriptors) == 1
    assert descriptors[0] == WALLET_DESCRIPTOR_EXTERNAL
