# SPDX-License-Identifier: MIT OR Apache-2.0

"""
tests/test_framework/electrum/base.py

Base client to connect to Floresta's Electrum server.
"""

import json
import socket
from typing import Any, List, Tuple
from OpenSSL import SSL

from test_framework.electrum import ConfigElectrum


# pylint: disable=too-few-public-methods
class BaseClient:
    """
    Helper class to connect to Floresta's Electrum server.
    """

    def __init__(self, config: ConfigElectrum, log):
        self._conn = None
        self._config = config
        self._log = log

    @property
    def log(self):
        """Getter for `log` property"""
        return self._log

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

    @property
    def tls(self) -> bool:
        """
        Check if the client is using TLS.
        """
        return self._config.tls is not None

    @property
    def port(self) -> int:
        """
        Get the port for the Electrum client.
        """
        if self.tls:
            return self._config.tls.port

        return self._config.port

    def set_config(self, config: ConfigElectrum):
        """Set the config for the Electrum client"""
        self._config = config

    def create_connection(self):
        """
        Create a connection to the server.
        """
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect((self._config.host, self.port))

        if self.tls:
            context = SSL.Context(SSL.TLS_METHOD)
            context.set_verify(SSL.VERIFY_NONE, lambda *args: True)
            tls_conn = SSL.Connection(context, s)
            tls_conn.set_connect_state()
            tls_conn.do_handshake()  # Perform the TLS handshake
            self._conn = tls_conn
        else:
            self._conn = s

    def read_line(self) -> str:
        """
        Read a single newline terminated message from the server.
        """
        response = b""
        while True:
            chunk = self.conn.recv(1)
            if not chunk:
                break
            response += chunk
            if b"\n" in response:
                break

        response = response.decode("utf-8").strip()
        self.log.debug(response)

        return response

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
        self.log.debug(f"GET electrum://{mnt_point}?params={params}")
        self.conn.sendall(request.encode("utf-8") + b"\n")

        # The server pushes unsolicited notifications (a new block header, an
        # updated script hash) over this same socket, so skip anything that is
        # not a response before handing one back.
        while True:
            response = self.read_line()
            if not response:
                raise ConnectionError("Electrum server closed the connection")

            message = json.loads(response)
            if isinstance(message, dict) and "id" in message:
                return message

            self.log.debug(f"Skipping electrum notification: {response}")

    def batch_request(self, calls: List[Tuple[str, List[Any]]]) -> object:
        """
        Send batch JSON-RPC requests to electrum's server.
        """
        request_map = {
            i: {"jsonrpc": "2.0", "id": i, "method": method, "params": params}
            for i, (method, params) in enumerate(calls)
        }

        request_list = list(request_map.values())
        self.log.debug(
            "BATCH "
            + ", ".join(
                f"electrum://{'/'.join(m.split('.'))}?params={p}" for m, p in calls
            )
        )
        self.conn.sendall(json.dumps(request_list).encode("utf-8") + b"\n")

        # As in `request`, drop any notification that lands before the batch
        # response, which is the only message coming back as a list.
        while True:
            response = self.read_line()
            if not response:
                raise ConnectionError("Electrum server closed the connection")

            if isinstance(json.loads(response), list):
                return response

            self.log.debug(f"Skipping electrum notification: {response}")
