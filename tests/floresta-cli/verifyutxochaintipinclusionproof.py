# SPDX-License-Identifier: MIT OR Apache-2.0

"""
verifyutxochaintipinclusionproof.py

Functional tests for the `verifyutxochaintipinclusionproof` RPC command,
exercising every code branch:
  - valid proof with both verbosity levels (0 and 1)
  - multiple coinbase UTXOs proved in a single proof
  - invalid verbosity level
  - proof verified against an explicit blockhash (UTXO unspent → stays valid)
  - spent UTXO invalidates proof (returns false at new tip, true at original blockhash)
  - unknown blockhash (BlockNotFound error)
  - invalid hex input
  - invalid proof structure
"""

import time
from typing import Any

import pytest
from test_framework.constants import (
    GENESIS_BLOCK_HASH,
    JSONRPC_ERRCODE_BLOCK_NOT_FOUND,
    JSONRPC_ERRCODE_INVALID_PARAMS,
    WALLET_ADDRESS_PKH,
    WALLET_DESCRIPTOR_EXTERNAL_PKH,
    WALLET_DESCRIPTOR_PRIV_EXTERNAL_PKH,
)
from test_framework.node import NodeType

NUM_BLOCKS = 10


def get_coinbase_txid(utreexod, height):
    """Get the coinbase txid at a given block height from utreexod."""
    block_hash = utreexod.rpc.get_blockhash(height)
    block = utreexod.rpc.perform_request("getblock", [block_hash, 1])
    return block["tx"][0]


def check_verbose_0(florestad, proof_hex, expected, blockhash=None):
    """Verify a proof with verbosity=0 and assert the boolean result."""
    params = [proof_hex, 0]
    if blockhash is not None:
        params.append(blockhash)
    result = florestad.rpc.perform_request("verifyutxochaintipinclusionproof", params)
    assert result is expected, f"Expected {expected}, got {result}"
    return result


def check_verbose_1_fields(florestad, proof_hex, utreexod_proof, blockhash=None):
    """Verify a proof with verbosity=1, validate field types, and cross-check
    against the proof data returned by utreexod's proveutxochaintipinclusion.

    Returns the verbose result dict.
    """
    params = [proof_hex, 1]
    if blockhash is not None:
        params.append(blockhash)
    result = florestad.rpc.perform_request("verifyutxochaintipinclusionproof", params)

    # Must be a dict with the expected keys (field names match utreexod)
    assert isinstance(result, dict), f"Expected dict, got {type(result)}"
    for key in ("valid", "provedathash", "prooftargets", "proofhashes", "hashesproven"):
        assert key in result, f"Missing key {key!r} in verbose response"

    assert result["valid"] is True

    # proofhashes: non-empty list of 64-char hex strings
    assert len(result["proofhashes"]) > 0
    for h in result["proofhashes"]:
        assert isinstance(h, str) and len(h) == 64, f"bad proof hash: {h}"
        int(h, 16)  # raises ValueError if not valid hex

    # hashesproven: each must be a 64-char hex string
    for h in result["hashesproven"]:
        assert isinstance(h, str) and len(h) == 64, f"bad proven hash: {h}"
        int(h, 16)

    # Cross-check against utreexod's proof data: the targets, proof hashes,
    # and proven hashes from florestad's verbose response must match what
    # utreexod originally generated.
    assert result["prooftargets"] == utreexod_proof["prooftargets"], (
        f"prooftargets mismatch: florestad={result['prooftargets']}, "
        f"utreexod={utreexod_proof['prooftargets']}"
    )
    assert result["proofhashes"] == utreexod_proof["proofhashes"], (
        f"proofhashes mismatch: florestad={result['proofhashes']}, "
        f"utreexod={utreexod_proof['proofhashes']}"
    )
    assert result["hashesproven"] == utreexod_proof["hashesproven"], (
        f"hashesproven mismatch: florestad={result['hashesproven']}, "
        f"utreexod={utreexod_proof['hashesproven']}"
    )

    return result


@pytest.mark.rpc
class TestVerifyProofOnSharedChain:
    """Read-only tests that share a single synced chain at NUM_BLOCKS."""

    log: Any = None
    node_manager: Any = None
    florestad: Any = None
    utreexod: Any = None

    @pytest.fixture(autouse=True)
    def setup_chain(
        self, setup_logging, florestad_bitcoind_utreexod_with_chain, node_manager
    ):
        """Initialize a synced three-node network for read-only proof tests."""
        self.log = setup_logging
        self.node_manager = node_manager
        self.florestad, _, self.utreexod = florestad_bitcoind_utreexod_with_chain(
            NUM_BLOCKS
        )
        self.node_manager.wait_for_sync_nodes()

    def test_valid(self):
        """Valid proof returns True for verbosity=0 and a detailed object for verbosity=1."""
        txid = get_coinbase_txid(self.utreexod, 1)
        self.log.info(f"Generating proof for coinbase tx {txid} at vout 0")
        proof_response = self.utreexod.rpc.proveutxochaintipinclusion([txid], [0])
        proof_hex = proof_response["hex"]

        self.log.info("Verifying proof with verbosity=0")
        check_verbose_0(self.florestad, proof_hex, expected=True)

        self.log.info("Verifying proof with verbosity=1")
        result_v1 = check_verbose_1_fields(self.florestad, proof_hex, proof_response)

        best_hash = self.florestad.rpc.get_bestblockhash()
        assert result_v1["provedathash"] == best_hash

        # Single UTXO proved → exactly 1 target and 1 proven hash
        assert len(result_v1["prooftargets"]) == 1
        assert len(result_v1["hashesproven"]) == 1

    def test_multiple_coinbase_utxos(self):
        """Prove inclusion of multiple coinbase UTXOs from different blocks."""
        txid_1 = get_coinbase_txid(self.utreexod, 1)
        txid_2 = get_coinbase_txid(self.utreexod, 2)
        self.log.info(f"Proving coinbase UTXOs: {txid_1}:0 and {txid_2}:0")

        proof_response = self.utreexod.rpc.proveutxochaintipinclusion(
            [txid_1, txid_2], [0, 0]
        )
        proof_hex = proof_response["hex"]
        self.log.info(f"Got proof hex ({len(proof_hex)} chars)")

        check_verbose_0(self.florestad, proof_hex, expected=True)

        result_v1 = check_verbose_1_fields(self.florestad, proof_hex, proof_response)
        assert (
            len(result_v1["prooftargets"]) == 2
        ), f"Expected 2 targets for 2 UTXOs, got {len(result_v1['prooftargets'])}"
        assert (
            len(result_v1["hashesproven"]) == 2
        ), f"Expected 2 proven hashes, got {len(result_v1['hashesproven'])}"

    def test_invalid_verbosity(self):
        """Valid proof with verbosity=2 should fail."""
        txid = get_coinbase_txid(self.utreexod, 1)
        self.log.info(f"Generating proof for coinbase tx {txid}")
        proof_response = self.utreexod.rpc.proveutxochaintipinclusion([txid], [0])
        proof_hex = proof_response["hex"]

        self.log.info("Verifying proof with invalid verbosity=2")
        self.florestad.rpc.ensure_rpc_call_error(
            "verifyutxochaintipinclusionproof",
            [proof_hex, 2],
            expected_rpcerror_code=JSONRPC_ERRCODE_INVALID_PARAMS,
        )

    def test_with_explicit_blockhash(self):
        """Proof can be verified against an explicit blockhash."""
        tip_hash = self.florestad.rpc.get_bestblockhash()
        txid = get_coinbase_txid(self.utreexod, 1)
        self.log.info(f"Generating proof for coinbase tx {txid}")
        proof_response = self.utreexod.rpc.proveutxochaintipinclusion([txid], [0])
        proof_hex = proof_response["hex"]

        self.log.info(f"Verifying proof against explicit blockhash {tip_hash}")
        check_verbose_0(self.florestad, proof_hex, expected=True, blockhash=tip_hash)

    def test_unknown_blockhash(self):
        """Passing a blockhash that the node doesn't know about returns an error."""
        txid = get_coinbase_txid(self.utreexod, 1)
        proof_response = self.utreexod.rpc.proveutxochaintipinclusion([txid], [0])
        proof_hex = proof_response["hex"]

        # Use a plausible but non-existent blockhash (genesis hash with last
        # char flipped). This must parse as a valid BlockHash yet not match any
        # block the node knows about.
        fake_hash = GENESIS_BLOCK_HASH[:-1] + (
            "1" if GENESIS_BLOCK_HASH[-1] != "1" else "2"
        )
        self.log.info(f"Verifying proof against unknown blockhash {fake_hash}")
        self.florestad.rpc.ensure_rpc_call_error(
            "verifyutxochaintipinclusionproof",
            [proof_hex, 0, fake_hash],
            expected_rpcerror_code=JSONRPC_ERRCODE_BLOCK_NOT_FOUND,
        )


@pytest.mark.rpc
class TestVerifyProofInvalidInput:
    """Tests that only need a bare florestad node (no chain/utreexod)."""

    log: Any = None
    florestad: Any = None

    @pytest.fixture(autouse=True)
    def setup_node(self, setup_logging, florestad_node):
        """Initialize a bare florestad node for invalid-input tests."""
        self.log = setup_logging
        self.florestad = florestad_node

    def test_invalid_proof(self):
        """Well-formed hex that doesn't decode to a valid proof should fail."""
        garbage_proof = "aa" * 32
        self.florestad.rpc.ensure_rpc_call_error(
            "verifyutxochaintipinclusionproof",
            [garbage_proof],
            expected_rpcerror_code=JSONRPC_ERRCODE_INVALID_PARAMS,
        )

    def test_invalid_hex(self):
        """Non-hex input should fail."""
        self.florestad.rpc.ensure_rpc_call_error(
            "verifyutxochaintipinclusionproof",
            ["not_valid_hex!!"],
            expected_rpcerror_code=JSONRPC_ERRCODE_INVALID_PARAMS,
        )


@pytest.mark.rpc
class TestVerifyProofBlockhashSemantics:
    """Tests that mine extra blocks to exercise blockhash-specific verification."""

    log: Any = None
    node_manager: Any = None
    florestad: Any = None
    bitcoind: Any = None
    utreexod: Any = None

    @pytest.fixture(autouse=True)
    def setup_chain(
        self, setup_logging, florestad_bitcoind_utreexod_with_chain, node_manager
    ):
        """Initialize a synced three-node network for blockhash semantics tests."""
        self.log = setup_logging
        self.node_manager = node_manager
        self.florestad, self.bitcoind, self.utreexod = (
            florestad_bitcoind_utreexod_with_chain(NUM_BLOCKS)
        )
        self.node_manager.wait_for_sync_nodes()

    def test_proof_stays_valid_after_mining(self):
        """Proof stays valid at the original blockhash after more blocks are mined."""
        tip_hash = self.florestad.rpc.get_bestblockhash()
        txid = get_coinbase_txid(self.utreexod, 1)
        self.log.info(f"Generating proof for coinbase tx {txid}")
        proof_response = self.utreexod.rpc.proveutxochaintipinclusion([txid], [0])
        proof_hex = proof_response["hex"]

        # Mine more blocks — the UTXO is NOT spent, so the proof stays valid
        self.log.info("Mining more blocks (UTXO remains unspent)...")
        self.utreexod.rpc.generate(5)
        self.node_manager.wait_for_sync_nodes()

        # Proof still valid with original blockhash (leaf was never deleted)
        self.log.info(f"Verifying proof still valid with original blockhash {tip_hash}")
        check_verbose_0(self.florestad, proof_hex, expected=True, blockhash=tip_hash)


@pytest.mark.rpc
class TestVerifyProofSpentUtxo:
    """Test that spending a UTXO invalidates its proof at the new chain tip.

    Uses a legacy P2PKH mining address so that coinbases can be spent without
    segwit activation (utreexod activates segwit via BIP9 on regtest, which
    requires ~432 blocks). 110 blocks is enough: 100 for coinbase maturity
    plus a small buffer.
    """

    MATURE_CHAIN_HEIGHT = 110

    log: Any = None
    node_manager: Any = None
    florestad: Any = None
    bitcoind: Any = None
    utreexod: Any = None

    @pytest.fixture(autouse=True)
    def setup_chain(self, setup_logging, node_manager):
        """Initialize a three-node network with legacy PKH mining for spend tests."""
        self.log = setup_logging
        self.node_manager = node_manager

        # Build a three-node network with utreexod mining to a legacy P2PKH
        # address so we can spend coinbases without needing segwit.
        self.florestad = node_manager.add_node_default_args(variant=NodeType.FLORESTAD)
        self.bitcoind = node_manager.add_node_default_args(variant=NodeType.BITCOIND)
        self.utreexod = node_manager.add_node_extra_args(
            variant=NodeType.UTREEXOD,
            extra_args=[
                f"--miningaddr={WALLET_ADDRESS_PKH}",
                "--utreexoproofindex",
                "--prune=0",
            ],
        )
        node_manager.run_node(self.florestad)
        node_manager.run_node(self.bitcoind)
        node_manager.run_node(self.utreexod)

        # Load the PKH descriptor into florestad so it tracks our coinbases.
        self.florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL_PKH)

        # Mine blocks before connecting nodes (fast IBD sync).
        self.utreexod.rpc.generate(self.MATURE_CHAIN_HEIGHT)

        # Establish mesh connectivity.
        node_manager.connect_nodes(self.florestad, self.utreexod)
        time.sleep(3)
        node_manager.connect_nodes(self.bitcoind, self.utreexod)
        time.sleep(1)
        node_manager.connect_nodes(self.florestad, self.bitcoind)

        self.node_manager.wait_for_sync_nodes()

        # Create a wallet on bitcoind and import the PKH private descriptor
        # so bitcoind can sign spends of utreexod's coinbases.
        self.bitcoind.rpc.perform_request("createwallet", ["test_wallet"])
        self.bitcoind.rpc.perform_request(
            "importdescriptors",
            [[{"desc": WALLET_DESCRIPTOR_PRIV_EXTERNAL_PKH, "timestamp": "now"}]],
        )

    def test_spent_utxo_invalidates_proof(self):
        """
        A proof becomes invalid only when the proven UTXO is spent (leaf deleted
        from the accumulator). Mining new blocks alone does NOT invalidate a proof
        as long as the leaf still exists.
        """
        # Block 1's coinbase has >100 confirmations at height 110, so it's
        # spendable.
        txid = get_coinbase_txid(self.utreexod, 1)
        self.log.info(f"Generating proof for coinbase tx {txid}:0")
        proof_response = self.utreexod.rpc.proveutxochaintipinclusion([txid], [0])
        proof_hex = proof_response["hex"]
        original_tip = self.florestad.rpc.get_bestblockhash()

        # Proof is valid at the current tip
        self.log.info("Verifying proof is valid before spending...")
        result = self.florestad.rpc.perform_request(
            "verifyutxochaintipinclusionproof", [proof_hex, 0, original_tip]
        )
        assert result is True

        # Spend the exact proved UTXO via a raw transaction signed by bitcoind
        # (which has the private key) and broadcast to utreexod (which mines).
        dest_addr = self.bitcoind.rpc.perform_request("getnewaddress", [])
        raw_tx = self.bitcoind.rpc.perform_request(
            "createrawtransaction",
            [[{"txid": txid, "vout": 0}], {dest_addr: 49.99}],
        )
        signed = self.bitcoind.rpc.perform_request(
            "signrawtransactionwithwallet", [raw_tx]
        )
        self.log.info(f"Spending proved UTXO {txid}:0 to {dest_addr}")
        # Submit to utreexod so the tx is in its mempool for the next mined block
        spend_txid = self.utreexod.rpc.perform_request(
            "sendrawtransaction", [signed["hex"]]
        )
        self.log.info(f"Spend txid: {spend_txid}")

        # Mine a block to confirm the spend
        self.utreexod.rpc.generate(1)
        self.node_manager.wait_for_sync_nodes()

        # Proof at the original blockhash is still valid (the accumulator at that
        # point in time still had the leaf)
        self.log.info(
            "Verifying proof still valid at original blockhash (pre-spend)..."
        )
        result = self.florestad.rpc.perform_request(
            "verifyutxochaintipinclusionproof", [proof_hex, 0, original_tip]
        )
        assert result is True

        # Proof at the NEW tip should return false — the leaf has been deleted
        # from the accumulator, so stump.verify() returns Ok(false)
        new_tip = self.florestad.rpc.get_bestblockhash()
        self.log.info(
            f"Verifying proof returns false at new tip {new_tip} (post-spend)..."
        )
        result = self.florestad.rpc.perform_request(
            "verifyutxochaintipinclusionproof", [proof_hex, 0, new_tip]
        )
        assert result is False, f"Expected False after spend, got {result}"
