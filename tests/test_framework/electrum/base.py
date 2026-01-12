"""
tests/test_framework/electrum/base.py

Base client to connect to Floresta's Electrum server.
"""

import json
import socket
from datetime import datetime, timezone
from typing import Any, List, Tuple
from OpenSSL import SSL

from test_framework.electrum import ConfigElectrum


# pylint: disable=too-few-public-methods
class BaseClient:
    """
    Helper class to connect to Floresta's Electrum server.
    """

    def __init__(self, config: ConfigElectrum):
        self._conn = None
        self._host = config.host
        if config.tls is None:
            self._port = config.port
            self._tls = False
        else:
            self._port = config.tls.port
            self._tls = True

    @property
    def conn(self) -> socket.socket:
        """
        Return the socket connection
        """
        return self._conn

    @conn.setter
    def conn(self, value: socket.socket):
        """
        Set the socket connection
        """
        self._conn = value

    @property
    def is_connected(self) -> bool:
        """
        Check if the client is connected to the server.
        """
        return self._conn is not None

    def create_connection(self):
        """
        Create a connection to the server.
        """
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect((self._host, self._port))

        if self._tls:
            context = SSL.Context(SSL.TLS_METHOD)
            context.set_verify(SSL.VERIFY_NONE, lambda *args: True)
            tls_conn = SSL.Connection(context, s)
            tls_conn.set_connect_state()
            tls_conn.do_handshake()  # Perform the TLS handshake
            self._conn = tls_conn
        else:
            self._conn = s

    def log(self, msg):
        """
        Log a message to the console
        """
        print(f"[{self.__class__.__name__.upper()} {datetime.now(timezone.utc)}] {msg}")

    def request(self, method, params) -> object:
        """
        Request something to Floresta server
        """
        if not self.is_connected:
            self.create_connection()
            if not self.is_connected:
                raise ConnectionError("Could not connect to Electrum server")

        request = json.dumps(
            {"jsonrpc": "2.0", "id": 0, "method": method, "params": params}
        )

        mnt_point = "/".join(method.split("."))
        self.log(f"GET electrum://{mnt_point}?params={params}")
        self.conn.sendall(request.encode("utf-8") + b"\n")

        response = b""
        while True:
            chunk = self.conn.recv(1)
            if not chunk:
                break
            response += chunk
            if b"\n" in response:
                break
        response = response.decode("utf-8").strip()
        self.log(response)

        return json.loads(response)

    def batch_request(self, calls: List[Tuple[str, List[Any]]]) -> object:
        """
        Send batch JSON-RPC requests to electrum's server.
        """
        request_map = {
            i: {"jsonrpc": "2.0", "id": i, "method": method, "params": params}
            for i, (method, params) in enumerate(calls)
        }

        request_list = list(request_map.values())
        self.log(
            "BATCH "
            + ", ".join(
                f"electrum://{'/'.join(m.split('.'))}?params={p}" for m, p in calls
            )
        )
        self.conn.sendall(json.dumps(request_list).encode("utf-8") + b"\n")

        response = b""
        while True:
            chunk = self.conn.recv(1)
            if not chunk:
                break
            response += chunk
            if b"\n" in response:
                break

        response = response.decode("utf-8").strip()
        self.log(response)
        return response
