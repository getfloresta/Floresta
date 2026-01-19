"""
Test the --connect cli option of florestad

This test will start a utreexod, then start a florestad node with
the --connect option pointing to the utreexod node. Then check if
the utreexod node is connected to the florestad node.
"""

import time
import pytest


@pytest.mark.florestad
def test_connect(utreexod_node, add_node_with_extra_args):
    """
    Test the --connect flag of florestad.
    """
    utreexod = utreexod_node
    _florestad = add_node_with_extra_args(
        variant="florestad",
        extra_args=[f"--connect={utreexod.get_host()}:{utreexod.get_port('p2p')}"],
    )

    end = time.time() + 10

    while time.time() < end:
        res = utreexod.rpc.get_peerinfo()
        if len(res) == 1:
            return

    pytest.fail("Florestad node did not connect to Utreexod node in time")
