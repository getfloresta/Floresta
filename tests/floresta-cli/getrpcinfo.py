"""
floresta_cli_getrpcinfo.py

This functional test cli utility to interact with a Floresta node with `getrpcinfo`
"""

import pytest


@pytest.mark.rpc
def test_get_rpc_info(florestad_node):
    """
    Test the `getrpcinfo` RPC call for the `florestad` node.
    """
    result = florestad_node.rpc.get_rpcinfo()
    expected_logpath = "/regtest/debug.log"

    # Assert the structure of the response
    assert set(result.keys()) == {"active_commands", "logpath"}
    assert len(result["active_commands"]) == 1

    command = result["active_commands"][0]
    assert set(command.keys()) == {"duration", "method"}
    assert command["method"] == "getrpcinfo"
    assert command["duration"] > 0
    assert expected_logpath in result["logpath"]
