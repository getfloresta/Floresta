# pylint: disable=all
#!/usr/bin/env python3
# Copyright (c) 2022-present The Bitcoin Core developers
# Distributed under the MIT software license, see the accompanying
# file COPYING or http://www.opensource.org/licenses/mit-license.php.

"""Test-only implementation of Poly1305 authenticator

It is designed for ease of understanding, not performance.

WARNING: This code is slow and trivially vulnerable to side channel attacks. Do not use for
anything but tests.
"""

import unittest


class Poly1305:
    """Class representing a running poly1305 computation."""

    MODULUS = 2**130 - 5

    def __init__(self, key):
        self.r = int.from_bytes(key[:16], "little") & 0xFFFFFFC0FFFFFFC0FFFFFFC0FFFFFFF
        self.s = int.from_bytes(key[16:], "little")

    def tag(self, data):
        """Compute the poly1305 tag."""
        acc, length = 0, len(data)
        for i in range((length + 15) // 16):
            chunk = data[i * 16 : min(length, (i + 1) * 16)]
            val = int.from_bytes(chunk, "little") + 256 ** len(chunk)
            acc = (self.r * (acc + val)) % Poly1305.MODULUS
        return ((acc + self.s) & 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF).to_bytes(
            16, "little"
        )
