# pylint: disable=all
#!/usr/bin/env python3
# Copyright (c) 2022-present The Bitcoin Core developers
# Distributed under the MIT software license, see the accompanying
# file COPYING or http://www.opensource.org/licenses/mit-license.php.

"""Test-only implementation of ChaCha20 Poly1305 AEAD Construction in RFC 8439 and FSChaCha20Poly1305 for BIP 324

It is designed for ease of understanding, not performance.

WARNING: This code is slow and trivially vulnerable to side channel attacks. Do not use for
anything but tests.
"""

import unittest

from .chacha20 import chacha20_block, REKEY_INTERVAL
from .poly1305 import Poly1305


def pad16(x):
    if len(x) % 16 == 0:
        return b""
    return b"\x00" * (16 - (len(x) % 16))


def aead_chacha20_poly1305_encrypt(key, nonce, aad, plaintext):
    """Encrypt a plaintext using ChaCha20Poly1305."""
    if plaintext is None:
        return None
    ret = bytearray()
    msg_len = len(plaintext)
    for i in range((msg_len + 63) // 64):
        now = min(64, msg_len - 64 * i)
        keystream = chacha20_block(key, nonce, i + 1)
        for j in range(now):
            ret.append(plaintext[j + 64 * i] ^ keystream[j])
    poly1305 = Poly1305(chacha20_block(key, nonce, 0)[:32])
    mac_data = aad + pad16(aad)
    mac_data += ret + pad16(ret)
    mac_data += len(aad).to_bytes(8, "little") + msg_len.to_bytes(8, "little")
    ret += poly1305.tag(mac_data)
    return bytes(ret)


def aead_chacha20_poly1305_decrypt(key, nonce, aad, ciphertext):
    """Decrypt a ChaCha20Poly1305 ciphertext."""
    if ciphertext is None or len(ciphertext) < 16:
        return None
    msg_len = len(ciphertext) - 16
    poly1305 = Poly1305(chacha20_block(key, nonce, 0)[:32])
    mac_data = aad + pad16(aad)
    mac_data += ciphertext[:-16] + pad16(ciphertext[:-16])
    mac_data += len(aad).to_bytes(8, "little") + msg_len.to_bytes(8, "little")
    if ciphertext[-16:] != poly1305.tag(mac_data):
        return None
    ret = bytearray()
    for i in range((msg_len + 63) // 64):
        now = min(64, msg_len - 64 * i)
        keystream = chacha20_block(key, nonce, i + 1)
        for j in range(now):
            ret.append(ciphertext[j + 64 * i] ^ keystream[j])
    return bytes(ret)


class FSChaCha20Poly1305:
    """Rekeying wrapper AEAD around ChaCha20Poly1305."""

    def __init__(self, initial_key):
        self._key = initial_key
        self._packet_counter = 0

    def _crypt(self, aad, text, is_decrypt):
        nonce = (self._packet_counter % REKEY_INTERVAL).to_bytes(4, "little") + (
            self._packet_counter // REKEY_INTERVAL
        ).to_bytes(8, "little")
        if is_decrypt:
            ret = aead_chacha20_poly1305_decrypt(self._key, nonce, aad, text)
        else:
            ret = aead_chacha20_poly1305_encrypt(self._key, nonce, aad, text)
        if (self._packet_counter + 1) % REKEY_INTERVAL == 0:
            rekey_nonce = b"\xff\xff\xff\xff" + nonce[4:]
            self._key = aead_chacha20_poly1305_encrypt(
                self._key, rekey_nonce, b"", b"\x00" * 32
            )[:32]
        self._packet_counter += 1
        return ret

    def decrypt(self, aad, ciphertext):
        return self._crypt(aad, ciphertext, True)

    def encrypt(self, aad, plaintext):
        return self._crypt(aad, plaintext, False)
