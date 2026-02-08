"""
RPC configuration module
"""

from typing import Optional


# pylint: disable=too-few-public-methods
class ConfigRPC:
    """
    Configuration for RPC connection
    """

    def __init__(
        self,
        host: str,
        port: int,
        user: Optional[str] = None,
        password: Optional[str] = None,
    ):
        self.host = host
        self.port = port
        self.user = user
        self.password = password
