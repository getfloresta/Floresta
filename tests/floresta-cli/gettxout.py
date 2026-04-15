# SPDX-License-Identifier: MIT OR Apache-2.0

"""
gettxout.py

This functional test cli utility to interact with a Floresta node with `getxtout` command.
"""

import time
import pytest

from test_framework.constants import WALLET_DESCRIPTOR_EXTERNAL

TIMEOUT_SECONDS = 120


# pylint: disable=too-many-locals
@pytest.mark.rpc
def test_get_txout(setup_logging, florestad_bitcoind_utreexod_with_chain):
    """
    Test the `gettxout` command for a specific transaction output.
    """
    log = setup_logging
    blocks = 10
    florestad, bitcoind, utreexod = florestad_bitcoind_utreexod_with_chain(blocks)

    log.info(
        f"Loading descriptor into Floresta node wallet: {WALLET_DESCRIPTOR_EXTERNAL}"
    )
    result = florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)
    assert result

    peer_info = bitcoind.rpc.get_peerinfo()
    peer_id = next(peer["id"] for peer in peer_info if "utreexo" in peer["subver"])
    best_block_hash = utreexod.rpc.get_blockhash(blocks)

    log.info("Waiting for Floresta and Bitcoind to sync with Utreexod...")
    timeout = time.time() + TIMEOUT_SECONDS
    while time.time() < timeout:
        floresta_info = florestad.rpc.get_blockchain_info()
        if (
            floresta_info["headers"]
            == utreexod.rpc.get_block_count()
            == bitcoind.rpc.get_block_count()
            == blocks
            and not floresta_info["initialblockdownload"]
        ):
            break

        time.sleep(1)
        # Forcing a re-fetch of the block from the peer
        try:
            bitcoind.rpc.get_block_from_peer(best_block_hash, peer_id)
        # pylint: disable=broad-exception-caught
        except Exception as e:
            log.error(f"Error fetching block from peer: {e}")

    assert (
        floresta_info["headers"] == blocks and not floresta_info["initialblockdownload"]
    )

    log.info("Comparing gettxout results between Floresta and Bitcoind...")
    for height in range(2, blocks):
        block_hash = florestad.rpc.get_blockhash(height)
        block = florestad.rpc.get_block(block_hash)
        log.info(f"Comparing gettxout results for {height} block {block_hash}...")

        for tx in block["tx"]:
            txout_floresta = florestad.rpc.get_txout(tx, vout=0, include_mempool=False)

            assert txout_floresta is not None, f"Txout for tx {tx} is None in Floresta."

            txout_bitcoind = bitcoind.rpc.get_txout(tx, vout=0, include_mempool=False)
            assert txout_bitcoind is not None, f"Txout for tx {tx} is None in Bitcoind."

            for key in txout_bitcoind.keys():
                if key in ["bestblock", "confirmations"]:
                    continue

                if key == "scriptPubKey":
                    for subkey in txout_bitcoind["scriptPubKey"].keys():
                        log.debug(f"Comparing scriptPubKey[{subkey}] for tx {tx}...")
                        assert (
                            txout_floresta["scriptPubKey"][subkey]
                            == txout_bitcoind["scriptPubKey"][subkey]
                        )
                else:
                    log.debug(f"Comparing {key} for tx {tx}...")
                    assert txout_floresta[key] == txout_bitcoind[key]
