"""
Electrum configuration for tests
"""

from typing import Optional


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
