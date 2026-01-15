"""
tests/test_framework/electrum/base.py

Base client to connect to Floresta's Electrum server.
"""

import json
import socket
from typing import Any, List, Tuple
from OpenSSL import SSL


# pylint: disable=too-few-public-methods
class BaseClient:
    """
    Helper class to connect to Floresta's Electrum server.
    """

    def __init__(self, log, host, port=8080, tls=False):
        self.log = log
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect((host, port))

        if tls:
            context = SSL.Context(SSL.TLS_METHOD)
            context.set_verify(SSL.VERIFY_NONE, lambda *args: True)
            tls_conn = SSL.Connection(context, s)
            tls_conn.set_connect_state()
            tls_conn.do_handshake()  # Perform the TLS handshake
            self._conn = tls_conn
        else:
            self._conn = s

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

    # pylint: disable=R0801
    def log_msg(self, message: str):
        """Format a log message for the console"""
        return f"[{self.__class__.__name__.upper()}] {message}"

    def request(self, method, params) -> object:
        """
        Request something to Floresta server
        """
        request = json.dumps(
            {"jsonrpc": "2.0", "id": 0, "method": method, "params": params}
        )

        mnt_point = "/".join(method.split("."))
        self.log.debug(self.log_msg(f"GET electrum://{mnt_point}?params={params}"))
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
        self.log.debug(self.log_msg(response))

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
        self.log.debug(
            self.log_msg(
                "BATCH "
                + ", ".join(
                    f"electrum://{'/'.join(m.split('.'))}?params={p}" for m, p in calls
                )
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
        self.log.debug(self.log_msg(response))
        return response
