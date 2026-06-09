# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getnetworkinfo.py

This functional test exercises the `getnetworkinfo` RPC against a freshly
started Floresta node and asserts that the response is shaped like
Bitcoin Core's `getnetworkinfo` and reflects Floresta-specific defaults.
"""

import pytest

EXPECTED_KEYS = {
    "version",
    "subversion",
    "protocolversion",
    "localservices",
    "localservicesnames",
    "localrelay",
    "timeoffset",
    "connections",
    "connections_in",
    "connections_out",
    "networkactive",
    "networks",
    "relayfee",
    "incrementalfee",
    "localaddresses",
    "warnings",
}

EXPECTED_NETWORK_NAMES = ["ipv4", "ipv6", "onion", "i2p", "cjdns"]


@pytest.mark.rpc
def test_get_network_info(florestad_node):
    """
    Test that `getnetworkinfo` returns a Core-compatible object on a
    fresh node with no peers and no proxy configured.
    """
    info = florestad_node.rpc.get_networkinfo()

    # All Core-mandated keys must be present.
    assert (
        set(info.keys()) == EXPECTED_KEYS
    ), f"unexpected keys: {set(info.keys()) ^ EXPECTED_KEYS}"

    # Protocol version is fixed at 70016.
    assert info["protocolversion"] == 70016

    # Floresta version, encoded as Core's MMmmpp, must be a positive integer.
    assert isinstance(info["version"], int)
    assert info["version"] > 0

    # BIP-14 user-agent: slash-wrapped Floresta string.
    assert info["subversion"].startswith("/")
    assert info["subversion"].endswith("/")
    assert "Floresta" in info["subversion"]

    # localservices is a 16-char hex string with at least WITNESS+P2P_V2+UTREEXO bits set.
    assert isinstance(info["localservices"], str)
    assert len(info["localservices"]) == 16
    int(info["localservices"], 16)  # parseable

    # localservicesnames must include the Floresta-advertised flags.
    assert "WITNESS" in info["localservicesnames"]
    assert "P2P_V2" in info["localservicesnames"]
    assert "UTREEXO" in info["localservicesnames"]

    # Floresta does not relay txs and does not toggle networking.
    assert info["localrelay"] is False
    assert info["networkactive"] is True

    # Fresh node => no peers, all outbound counters zero.
    assert info["connections"] == 0
    assert info["connections_in"] == 0
    assert info["connections_out"] == 0

    # Floresta doesn't track these — must be present, must be zero/empty.
    assert info["timeoffset"] == 0
    assert info["relayfee"] == 0
    assert info["incrementalfee"] == 0
    assert info["localaddresses"] == []
    assert info["warnings"] == []

    # All 5 Core networks must be listed, in the documented order.
    assert [n["name"] for n in info["networks"]] == EXPECTED_NETWORK_NAMES

    # No proxy configured by default => onion/i2p unreachable, ipv4/ipv6 reachable,
    # cjdns always unreachable. proxy string must be empty for every entry.
    expected_reachable = {
        "ipv4": True,
        "ipv6": True,
        "onion": True,
        "i2p": False,
        "cjdns": False,
    }
    for net in info["networks"]:
        assert (
            net["reachable"] is expected_reachable[net["name"]]
        ), f"{net['name']}: reachable={net['reachable']}"
        assert net["limited"] is not net["reachable"]
        assert net["proxy"] == ""
        assert net["proxy_randomize_credentials"] is False
