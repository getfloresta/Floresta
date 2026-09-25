# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Electrum configuration for tests
"""

import socket
import time
from typing import Optional


def wait_on_socket(host, port, timeout=10, expect_open=True):
    """Poll until the port is open (or closed) or timeout."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection((host, port), timeout=0.5):
                if expect_open:
                    return True
        except (ConnectionRefusedError, OSError):
            if not expect_open:
                return True
        time.sleep(0.2)
    return False


# pylint: disable=too-few-public-methods
class ConfigTls:
    """
    Configuration for TLS connection
    """

    def __init__(self, cert_file: str, key_file: str, port: int):
        self.cert_file = cert_file
        self.key_file = key_file
        self.port = port


# pylint: disable=too-few-public-methods
class ConfigElectrum:
    """
    Configuration for Electrum connection
    """

    def __init__(self, host: str, port: int, tls: Optional[ConfigTls]):
        self.host = host
        self.port = port
        self.tls = tls
