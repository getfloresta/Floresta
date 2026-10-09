# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Compare Floresta's Electrum server with romanz/electrs and ElectrumX.

The three servers index the same regtest chain. A field is compared only
where electrs and ElectrumX agree with each other: where they do not, the
test logs it and moves on. The tests skip unless `tests/prepare.sh
--electrum-compat` fetched electrs and the `electrum-compat` extra
installed ElectrumX.
"""

import hashlib
import random
from types import SimpleNamespace

import pytest
from test_framework.constants import (
    WALLET_DESCRIPTOR_EXTERNAL,
    WALLET_DESCRIPTOR_INTERNAL,
    WALLET_DESCRIPTOR_PRIV_EXTERNAL,
    WALLET_DESCRIPTOR_PRIV_INTERNAL,
)
from test_framework.electrum import reference
from test_framework.node import NodeType
from test_framework.util import wait_until

# Addresses compared per descriptor, some of them never used
ADDRESSES = 4


def script_hash(bitcoind, address: str) -> str:
    """The Electrum script hash: sha256 of the scriptPubKey, byte-reversed."""
    spk = bitcoind.rpc.perform_request("validateaddress", [address])["scriptPubKey"]
    return hashlib.sha256(bytes.fromhex(spk)).digest()[::-1].hex()


def history_txids(client, sh: str) -> list[str]:
    """The txids in the history of script hash `sh`."""
    history = client.request("blockchain.scripthash.get_history", [sh])
    return [entry["tx_hash"] for entry in history]


def create_wallet(bitcoind) -> str:
    """
    Give bitcoind a wallet holding the keys Floresta watches, and return an
    address of that wallet that Floresta does not watch.
    """
    wallet = bitcoind.rpc.perform_request
    wallet("createwallet", ["compat"])
    wallet(
        "importdescriptors",
        [
            [
                {"desc": desc, "active": True, "internal": internal, "timestamp": "now"}
                for desc, internal in (
                    (WALLET_DESCRIPTOR_PRIV_EXTERNAL, False),
                    (WALLET_DESCRIPTOR_PRIV_INTERNAL, True),
                )
            ]
        ],
    )
    # Not taproot, which utreexod's regtest never activates
    return wallet("getnewaddress", ["", "legacy"])


def mine(manager, bitcoind, utreexod, txid: str):
    """Mine `txid`, which bitcoind's wallet made but did not send, with utreexod."""
    raw = bitcoind.rpc.perform_request("gettransaction", [txid])["hex"]
    utreexod.rpc.perform_request("sendrawtransaction", [raw])
    utreexod.rpc.generate(1)
    manager.wait_for_sync_nodes(is_finished_ibd=False)


def make_transactions(manager, bitcoind, utreexod, miner: str) -> tuple[list[str], str]:
    """
    Mine two blocks with one wallet transaction each: a payment to two
    watched addresses, then a spend from the first. Return the watched
    addresses and the spend.
    """
    wallet = bitcoind.rpc.perform_request
    watched = [
        address
        for desc in (WALLET_DESCRIPTOR_EXTERNAL, WALLET_DESCRIPTOR_INTERNAL)
        for address in wallet("deriveaddresses", [desc, [0, ADDRESSES - 1]])
    ]

    payment = wallet("send", [{watched[0]: 1, watched[1]: 2}])
    mine(manager, bitcoind, utreexod, payment["txid"])

    utxo = wallet("listunspent", [1, 9999, [watched[0]]])[0]
    spend = wallet(
        "send",
        [
            {miner: 0.5},
            None,
            "unset",
            None,
            {
                "inputs": [{"txid": utxo["txid"], "vout": utxo["vout"]}],
                "change_type": "bech32",
            },
        ],
    )
    mine(manager, bitcoind, utreexod, spend["txid"])

    return watched, spend["txid"]


def build_chain(manager):
    """
    Start florestad, bitcoind and utreexod on one chain, with the transactions
    of `make_transactions`. Return florestad, bitcoind, the watched addresses
    and the last transaction.
    """
    florestad = manager.add_node_default_args(variant=NodeType.FLORESTAD)
    manager.run_node(florestad)
    florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)
    florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_INTERNAL)

    bitcoind = manager.add_node_extra_args(
        variant=NodeType.BITCOIND,
        extra_args=["-txindex=1", "-fallbackfee=0.0001", "-walletbroadcast=0"],
    )
    manager.run_node(bitcoind)
    miner = create_wallet(bitcoind)

    utreexod = manager.add_node_extra_args(
        variant=NodeType.UTREEXOD, extra_args=[f"--miningaddr={miner}", "--prune=0"]
    )
    manager.run_node(utreexod)

    # Mine before connecting, so that the peers see the chain at handshake.
    # utreexod's regtest activates segwit at height 432.
    utreexod.rpc.generate(432)
    manager.connect_nodes(florestad, utreexod)
    manager.connect_nodes(bitcoind, utreexod)
    # Longer than the default wait: bitcoind took over 30 s to sync, and a
    # block mined while Floresta was still in IBD did not reach it
    wait_until(
        lambda: manager.check_sync_nodes(is_finished_ibd=True),
        timeout=180,
        error_msg="the nodes did not sync",
    )

    watched, last_txid = make_transactions(manager, bitcoind, utreexod, miner)
    return florestad, bitcoind, watched, last_txid


@pytest.fixture(scope="class", name="servers")
def fixture_servers(shared_node_manager, shared_setup_logging, request):
    """
    florestad, bitcoind and utreexod on one chain, electrs and ElectrumX
    indexing bitcoind, and a client to each Electrum server.
    """
    if reference.electrs_path() is None:
        pytest.skip("no electrs: run tests/prepare.sh --electrum-compat")
    if not reference.electrumx_installed():
        pytest.skip("no ElectrumX: run uv sync --extra electrum-compat")

    florestad, bitcoind, watched, last_txid = build_chain(shared_node_manager)
    height = bitcoind.rpc.get_block_count()
    script_hashes = [script_hash(bitcoind, address) for address in watched]

    started = []
    try:
        for start in (reference.start_electrs, reference.start_electrumx):
            started.append(start(bitcoind, request.node.name, shared_setup_logging))
            started[-1].wait_for_height(height)

        floresta, floresta_version = reference.connect(
            florestad.daemon.electrum_config.port, shared_setup_logging
        )
        # Floresta's Electrum server can lag its chain by seconds. It takes in
        # new blocks only after a second without requests: poll slower.
        wait_until(
            lambda: last_txid in history_txids(floresta, script_hashes[0]),
            interval=2,
            error_msg="Floresta's Electrum server did not see the last block",
        )

        yield SimpleNamespace(
            log=shared_setup_logging,
            florestad=florestad,
            height=height,
            script_hashes=script_hashes,
            clients=[floresta] + [server.client for server in started],
            versions=[floresta_version] + [server.version for server in started],
        )
    finally:
        for server in started:
            server.stop()


def check(servers, what: str, floresta, electrs, electrumx):
    """Assert Floresta matches the references, where they agree."""
    if electrs != electrumx:
        servers.log.warning(
            f"{what}: electrs and ElectrumX disagree, not compared: "
            f"{electrs} vs {electrumx}"
        )
        return

    assert floresta == electrs, f"{what}: Floresta {floresta}, references {electrs}"


def compare(servers, method: str, params: list, normalize=lambda result: result):
    """Send one request to the three servers and compare the results."""
    floresta, electrs, electrumx = (
        normalize(client.request(method, params)) for client in servers.clients
    )
    what = f"{method}{params}"

    if isinstance(electrs, dict) and isinstance(electrumx, dict):
        assert isinstance(floresta, dict), f"{what}: Floresta {floresta}"
        for key in sorted(electrs.keys() | electrumx.keys()):
            check(
                servers,
                f"{what}.{key}",
                floresta.get(key),
                electrs.get(key),
                electrumx.get(key),
            )
    else:
        check(servers, what, floresta, electrs, electrumx)


@pytest.mark.electrum
class TestCompareServers:
    """Compare Floresta's Electrum answers with electrs and ElectrumX."""

    def test_server_version(self, servers):
        """All three servers negotiate the version the client asks for."""
        assert servers.versions == [reference.PROTOCOL] * 3

    def test_headers(self, servers):
        """Tip, headers at chosen heights, and a run of headers from genesis."""
        compare(servers, "blockchain.headers.subscribe", [])

        for height in (0, random.randint(1, servers.height - 1), servers.height):
            compare(servers, "blockchain.block.header", [height])

        compare(servers, "blockchain.block.headers", [0, servers.height + 1])

    def test_scripthash(self, servers):
        """Balance, history, unspent outputs and status of each watched script."""
        for sh in servers.script_hashes:
            compare(servers, "blockchain.scripthash.get_balance", [sh])
            compare(servers, "blockchain.scripthash.get_history", [sh])
            compare(
                servers,
                "blockchain.scripthash.listunspent",
                [sh],
                normalize=lambda utxos: sorted(
                    utxos, key=lambda utxo: (utxo["tx_hash"], utxo["tx_pos"])
                ),
            )
            compare(servers, "blockchain.scripthash.subscribe", [sh])

    def test_transactions(self, servers):
        """Raw transaction and merkle branch of every transaction in the histories."""
        electrs = servers.clients[1]
        history = {
            (entry["tx_hash"], entry["height"])
            for sh in servers.script_hashes
            for entry in electrs.request("blockchain.scripthash.get_history", [sh])
        }
        assert history, "the watched scripts have no history"

        for txid, height in sorted(history):
            compare(servers, "blockchain.transaction.get", [txid])
            compare(servers, "blockchain.transaction.get_merkle", [txid, height])

    @pytest.mark.xfail(
        strict=True,
        raises=AssertionError,
        reason="server.version answers 1.4 while server.features says protocol_max 1.5",
    )
    def test_version_matches_features(self, servers):
        """Offered 1.4 to 1.5, Floresta picks the highest version it claims."""
        features = servers.clients[0].request("server.features", [])
        _, version = reference.connect(
            servers.florestad.daemon.electrum_config.port,
            servers.log,
            protocol=["1.4", "1.5"],
        )
        assert version == features["protocol_max"]
