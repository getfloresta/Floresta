# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Check that the Electrum server takes in a new block while clients poll it.

utreexod mines to WALLET_ADDRESS, which the loaded descriptor watches, so
each block adds one entry to that address's history. CLIENTS clients poll
that history every INTERVAL seconds while a block is mined. The history must
show the new block within WATCH seconds of florestad's RPC returning it as
best block, while the clients keep polling. The test then stops polling and
waits for the block, so that the rounds do not overlap.
"""

import hashlib
import threading
import time

import pytest
from test_framework.constants import WALLET_ADDRESS, WALLET_DESCRIPTOR_EXTERNAL
from test_framework.electrum.client import ElectrumClient
from test_framework.util import wait_until

CHARSET = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"
CLIENTS = 4
INTERVAL = 0.2
ROUNDS = 4
WATCH = 30.0


def p2wpkh_script_hash(address: str) -> str:
    """Electrum script hash of a v0 bech32 address (no checksum check)."""
    data = [CHARSET.index(c) for c in address[address.rfind("1") + 1 : -6]]
    acc, bits, program = 0, 0, []
    for value in data[1:]:
        acc = (acc << 5) | value
        bits += 5
        while bits >= 8:
            bits -= 8
            program.append((acc >> bits) & 0xFF)
    return hashlib.sha256(bytes([0, len(program)] + program)).digest()[::-1].hex()


class Poller(threading.Thread):
    """Poll get_history every INTERVAL seconds and keep the last length."""

    def __init__(self, client, sh):
        super().__init__(daemon=True)
        self.client, self.sh = client, sh
        self.length = None
        self.polls = 0
        self.error = None
        self.stop = threading.Event()

    def run(self):
        try:
            while not self.stop.is_set():
                self.length = len(self.client.get_history(self.sh))
                self.polls += 1
                self.stop.wait(INTERVAL)
        except Exception as error:  # pylint: disable=broad-exception-caught
            self.error = error
        finally:
            if self.client.is_connected:
                self.client.conn.close()


class TestElectrumBlockFreshness:
    """The Electrum history follows the chain tip while clients poll it."""

    log = None
    florestad = None
    utreexod = None

    @pytest.mark.electrum
    def test_block_freshness(self, node_manager, setup_logging, florestad_utreexod):
        """Mine ROUNDS blocks, one at a time, while CLIENTS clients poll."""
        self.log = setup_logging
        self.florestad, self.utreexod = florestad_utreexod
        self.florestad.rpc.load_descriptor(WALLET_DESCRIPTOR_EXTERNAL)
        sh = p2wpkh_script_hash(WALLET_ADDRESS)

        node_manager.generate_blocks_and_sync(10)
        wait_until(
            lambda: len(self.florestad.electrum.get_history(sh)) == 10, interval=2
        )

        results = [self.measure(sh) for _ in range(ROUNDS)]
        assert all(result["fresh_while_polling"] for result in results), results

    def measure(self, sh: str) -> dict:
        """Mine one block while polling; return how long the history stayed stale."""
        electrum = self.florestad.electrum
        before = len(electrum.get_history(sh))
        pollers = [
            Poller(ElectrumClient(self.florestad.daemon.electrum_config, self.log), sh)
            for _ in range(CLIENTS)
        ]
        for poller in pollers:
            poller.start()
            time.sleep(INTERVAL / CLIENTS)
        time.sleep(2)

        block = self.utreexod.rpc.generate(1)[0]
        wait_until(
            lambda: self.florestad.rpc.get_bestblockhash() == block, interval=0.1
        )
        tip = time.monotonic()
        polls = sum(poller.polls for poller in pollers)

        def fresh():
            return any(
                poller.length is not None and poller.length > before
                for poller in pollers
            )

        while time.monotonic() - tip < WATCH and not fresh():
            time.sleep(0.05)
        result = {
            "fresh_while_polling": fresh(),
            "seconds_after_tip": round(time.monotonic() - tip, 2),
            "polls_after_tip": sum(poller.polls for poller in pollers) - polls,
        }
        for poller in pollers:
            poller.stop.set()
            poller.join()
        errors = [poller.error for poller in pollers if poller.error is not None]
        assert not errors, errors

        self.log.info(f"result: {result}")

        # Wait for the block once polling stops, so rounds do not overlap. Poll
        # slower than once a second, or this wait holds the block back too.
        stopped = time.monotonic()
        wait_until(
            lambda: len(electrum.get_history(sh)) > before, timeout=30, interval=2
        )
        self.log.info(f"fresh {time.monotonic() - stopped:.2f} s after polling stopped")
        return result
