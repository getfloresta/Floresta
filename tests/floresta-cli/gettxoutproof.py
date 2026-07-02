# SPDX-License-Identifier: MIT OR Apache-2.0

"""
gettxoutproof.py

Functional test for the `gettxoutproof` RPC command.
Compares the Merkle proof returned by Floresta against Bitcoin Core
and verifies it using Bitcoin Core's `verifytxoutproof`.

TODO: extend this with non-coinbase transactions.
TODO: implicit blockhash while requesting a proof for a cached transaction
"""

import pytest

# pylint: disable=redefined-outer-name


@pytest.fixture(scope="class")
def setup_nodes(
    shared_setup_logging,
    shared_florestad_bitcoind_utreexod_with_chain,
):
    """Set up logging and the three-node network synced to 10 blocks."""
    return shared_setup_logging, shared_florestad_bitcoind_utreexod_with_chain(10)


@pytest.mark.rpc
class TestGetTxOutProof:
    """Tests for the gettxoutproof RPC command."""

    def test_single_tx_blocks(self, setup_nodes):
        """
        Compare Floresta's merkle proofs against Bitcoin Core for
        single-tx (coinbase) blocks, verified via verifytxoutproof.
        """
        log, (florestad, bitcoind, _) = setup_nodes

        log.info("Testing single-tx blocks with explicit blockhash...")
        for height in range(0, 10):
            block_hash = florestad.rpc.get_blockhash(height)
            block = florestad.rpc.get_block(block_hash)
            txid = block["tx"][0]

            proof_floresta = florestad.rpc.get_txout_proof([txid], block_hash)
            proof_bitcoind = bitcoind.rpc.get_txout_proof([txid], block_hash)

            assert (
                proof_floresta == proof_bitcoind
            ), f"Merkle proof mismatch for tx {txid} at height {height}"

            verified = bitcoind.rpc.verify_txout_proof(proof_floresta)
            assert (
                txid in verified
            ), f"Bitcoin Core could not verify Floresta's proof for tx {txid}"

        log.info("All single-tx proof checks passed.")

    def test_wrong_block(self, setup_nodes):
        """gettxoutproof errors when a txid is not in the specified block."""
        log, (florestad, _, _) = setup_nodes

        block_hash_2 = florestad.rpc.get_blockhash(2)
        block_2 = florestad.rpc.get_block(block_hash_2)
        txid_from_block_2 = block_2["tx"][0]

        block_hash_3 = florestad.rpc.get_blockhash(3)

        log.info("Requesting proof for tx from block 2 in block 3...")

        florestad.rpc.ensure_rpc_call_error(
            "gettxoutproof", params=[[txid_from_block_2], block_hash_3]
        )
        log.info("Correctly returned error for tx not in specified block.")

    def test_nonexistent_txid(self, setup_nodes):
        """gettxoutproof errors when given a txid not in the block."""
        log, (florestad, _, _) = setup_nodes

        fake_txid = "0000000000000000000000000000000000000000000000000000000000000001"
        block_hash = florestad.rpc.get_blockhash(2)

        log.info("Requesting proof for nonexistent txid...")

        florestad.rpc.ensure_rpc_call_error(
            "gettxoutproof", params=[[fake_txid], block_hash]
        )
        log.info("Correctly returned error for nonexistent txid.")
