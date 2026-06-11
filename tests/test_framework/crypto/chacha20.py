# pylint: disable=all
#!/usr/bin/env python3
# Copyright (c) 2022-present The Bitcoin Core developers
# Distributed under the MIT software license, see the accompanying
# file COPYING or http://www.opensource.org/licenses/mit-license.php.

"""Test-only implementation of ChaCha20 cipher and FSChaCha20 for BIP 324

It is designed for ease of understanding, not performance.

WARNING: This code is slow and trivially vulnerable to side channel attacks. Do not use for
anything but tests.
"""

import unittest

CHACHA20_INDICES = (
    (0, 4, 8, 12),
    (1, 5, 9, 13),
    (2, 6, 10, 14),
    (3, 7, 11, 15),
    (0, 5, 10, 15),
    (1, 6, 11, 12),
    (2, 7, 8, 13),
    (3, 4, 9, 14),
)

CHACHA20_CONSTANTS = (0x61707865, 0x3320646E, 0x79622D32, 0x6B206574)
REKEY_INTERVAL = 224  # packets


def rotl32(v, bits):
    """Rotate the 32-bit value v left by bits bits."""
    bits %= 32  # Make sure the term below does not throw an exception
    return ((v << bits) & 0xFFFFFFFF) | (v >> (32 - bits))


def chacha20_doubleround(s):
    """Apply a ChaCha20 double round to 16-element state array s.
    See https://cr.yp.to/chacha/chacha-20080128.pdf and https://tools.ietf.org/html/rfc8439
    """
    for a, b, c, d in CHACHA20_INDICES:
        s[a] = (s[a] + s[b]) & 0xFFFFFFFF
        s[d] = rotl32(s[d] ^ s[a], 16)
        s[c] = (s[c] + s[d]) & 0xFFFFFFFF
        s[b] = rotl32(s[b] ^ s[c], 12)
        s[a] = (s[a] + s[b]) & 0xFFFFFFFF
        s[d] = rotl32(s[d] ^ s[a], 8)
        s[c] = (s[c] + s[d]) & 0xFFFFFFFF
        s[b] = rotl32(s[b] ^ s[c], 7)


def chacha20_block(key, nonce, cnt):
    """Compute the 64-byte output of the ChaCha20 block function.
    Takes as input a 32-byte key, 12-byte nonce, and 32-bit integer counter.
    """
    # Initial state.
    init = [0] * 16
    init[:4] = CHACHA20_CONSTANTS[:4]
    init[4:12] = [int.from_bytes(key[i : i + 4], "little") for i in range(0, 32, 4)]
    init[12] = cnt
    init[13:16] = [int.from_bytes(nonce[i : i + 4], "little") for i in range(0, 12, 4)]
    # Perform 20 rounds.
    state = list(init)
    for _ in range(10):
        chacha20_doubleround(state)
    # Add initial values back into state.
    for i in range(16):
        state[i] = (state[i] + init[i]) & 0xFFFFFFFF
    # Produce byte output
    return b"".join(state[i].to_bytes(4, "little") for i in range(16))


class FSChaCha20:
    """Rekeying wrapper stream cipher around ChaCha20."""

    def __init__(self, initial_key, rekey_interval=REKEY_INTERVAL):
        self._key = initial_key
        self._rekey_interval = rekey_interval
        self._block_counter = 0
        self._chunk_counter = 0
        self._keystream = b""

    def _get_keystream_bytes(self, nbytes):
        while len(self._keystream) < nbytes:
            nonce = (0).to_bytes(4, "little") + (
                self._chunk_counter // self._rekey_interval
            ).to_bytes(8, "little")
            self._keystream += chacha20_block(self._key, nonce, self._block_counter)
            self._block_counter += 1
        ret = self._keystream[:nbytes]
        self._keystream = self._keystream[nbytes:]
        return ret

    def crypt(self, chunk):
        ks = self._get_keystream_bytes(len(chunk))
        ret = bytes([ks[i] ^ chunk[i] for i in range(len(chunk))])
        if ((self._chunk_counter + 1) % self._rekey_interval) == 0:
            self._key = self._get_keystream_bytes(32)
            self._block_counter = 0
            self._keystream = b""
        self._chunk_counter += 1
        return ret
