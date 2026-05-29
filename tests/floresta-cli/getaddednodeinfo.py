# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getaddednodeinfo.py

This functional test verifies the `getaddednodeinfo` RPC method.
"""

import pytest
from test_framework.constants import JSONRPC_ERRCODE_INVALID_PARAMS


@pytest.mark.rpc
class TestGetAddedNodeInfo:
    """Functional tests for the getaddednodeinfo RPC."""

    def test_get_added_node_info_empty(self, shared_florestad_node):
        """
        With no manually added nodes, getaddednodeinfo returns an empty list.
        """
        info = shared_florestad_node.rpc.get_added_node_info()
        assert isinstance(info, list)
        assert len(info) == 0

    def test_get_added_node_info_invalid_node_filter(self, shared_florestad_node):
        """
        Passing an invalid address as the node filter should return an error.
        """
        resp = shared_florestad_node.rpc.noraise_request(
            "getaddednodeinfo", ["not-a-valid-address"]
        )
        shared_florestad_node.rpc.assert_rpc_error(
            resp,
            expected_status_code=400,
            expected_rpcerror_code=JSONRPC_ERRCODE_INVALID_PARAMS,
            expected_message="Invalid network address provided",
        )

    def test_get_added_node_info_connected(
        self, shared_node_manager, shared_florestad_node, shared_bitcoind_node
    ):
        """
        After adding a node via addnode and connecting, getaddednodeinfo should
        report the node as connected with a populated addresses array.
        """
        shared_node_manager.connect_nodes(
            shared_florestad_node, shared_bitcoind_node, v2transport=True
        )

        info = shared_florestad_node.rpc.get_added_node_info()
        assert len(info) == 1

        entry = info[0]
        assert entry["addednode"] == shared_bitcoind_node.p2p_url
        assert entry["connected"] is True

        # The addresses array must be present and populated when connected
        assert "addresses" in entry
        assert len(entry["addresses"]) == 1
        assert entry["addresses"][0]["address"] == shared_bitcoind_node.p2p_url
        assert entry["addresses"][0]["connected"] == "outbound"

        # Cleanup: remove the manually added node
        shared_florestad_node.rpc.addnode(shared_bitcoind_node.p2p_url, "remove")
        info = shared_florestad_node.rpc.get_added_node_info()
        assert len(info) == 0

    def test_get_added_node_info_after_remove(
        self, shared_node_manager, shared_florestad_node, shared_bitcoind_node
    ):
        """
        After removing a manually added node, it should no longer appear in
        getaddednodeinfo.
        """
        shared_node_manager.connect_nodes(
            shared_florestad_node, shared_bitcoind_node, v2transport=True
        )

        info = shared_florestad_node.rpc.get_added_node_info()
        assert len(info) == 1

        shared_florestad_node.rpc.addnode(shared_bitcoind_node.p2p_url, "remove")

        info = shared_florestad_node.rpc.get_added_node_info()
        assert len(info) == 0

    def test_get_added_node_info_multiple_nodes(
        self, shared_node_manager, shared_florestad_node, shared_extra_bitcoind_pair
    ):
        """
        Adding multiple nodes via addnode should all appear in getaddednodeinfo.
        """
        bitcoind_a, bitcoind_b = shared_extra_bitcoind_pair

        shared_node_manager.connect_nodes(shared_florestad_node, bitcoind_a)
        shared_node_manager.connect_nodes(shared_florestad_node, bitcoind_b)

        info = shared_florestad_node.rpc.get_added_node_info()
        assert len(info) == 2

        added_addresses = {entry["addednode"] for entry in info}
        assert bitcoind_a.p2p_url in added_addresses
        assert bitcoind_b.p2p_url in added_addresses

        # Both should be connected with addresses populated
        for entry in info:
            assert entry["connected"] is True
            assert len(entry["addresses"]) == 1
            assert entry["addresses"][0]["connected"] == "outbound"

        # Cleanup: remove both manually added nodes
        shared_florestad_node.rpc.addnode(bitcoind_a.p2p_url, "remove")
        shared_florestad_node.rpc.addnode(bitcoind_b.p2p_url, "remove")
        info = shared_florestad_node.rpc.get_added_node_info()
        assert len(info) == 0

    def test_get_added_node_info_filter_by_node(
        self, shared_node_manager, shared_florestad_node, shared_extra_bitcoind_pair
    ):
        """
        The optional node parameter should filter results to a single entry.
        """
        bitcoind_a, bitcoind_b = shared_extra_bitcoind_pair

        shared_node_manager.connect_nodes(shared_florestad_node, bitcoind_a)
        shared_node_manager.connect_nodes(shared_florestad_node, bitcoind_b)

        # Filter for node A only
        filtered = shared_florestad_node.rpc.get_added_node_info(bitcoind_a.p2p_url)
        assert len(filtered) == 1
        assert filtered[0]["addednode"] == bitcoind_a.p2p_url

        # Filter for node B only
        filtered = shared_florestad_node.rpc.get_added_node_info(bitcoind_b.p2p_url)
        assert len(filtered) == 1
        assert filtered[0]["addednode"] == bitcoind_b.p2p_url

        # Filter for non-existent node returns empty list
        filtered_empty = shared_florestad_node.rpc.get_added_node_info("1.2.3.4:9999")
        assert len(filtered_empty) == 0

        # Cleanup: remove both manually added nodes
        shared_florestad_node.rpc.addnode(bitcoind_a.p2p_url, "remove")
        shared_florestad_node.rpc.addnode(bitcoind_b.p2p_url, "remove")
        info = shared_florestad_node.rpc.get_added_node_info()
        assert len(info) == 0

    def test_get_added_node_info_connect_flag_not_listed(
        self, shared_connect_flag_pair
    ):
        """
        Peers added via --connect should NOT appear in getaddednodeinfo.
        Only peers added via the addnode RPC are listed.
        """
        florestad, _ = shared_connect_flag_pair

        # The --connect peer must NOT appear in getaddednodeinfo
        info = florestad.rpc.get_added_node_info()
        assert isinstance(info, list)
        assert len(info) == 0
