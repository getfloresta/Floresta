# SPDX-License-Identifier: MIT OR Apache-2.0

"""
tests/test_framework/electrum/base.py

Base client to connect to Floresta's Electrum server.
"""

import json
import socket
from OpenSSL import SSL

from test_framework.electrum import ConfigElectrum
from test_framework.util import wait_until

# Read one byte at a time so the first ``\n`` terminates the message.
# Any data that arrives after it stays in the kernel socket buffer and
# will be returned by the next call.  This prevents coalesced messages
# (e.g. a notification + response in one TCP segment) from being
# concatenated into an invalid JSON payload.
BUFFER_SIZE = 1


# pylint: disable=too-few-public-methods
class BaseClient:
    """
    Helper class to connect to Floresta's Electrum server.
    """

    def __init__(self, config: ConfigElectrum, log):
        self._conn = None
        self._config = config
        self._log = log
        self._request_id = 0
        self.result = None

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

    def receive_response(self) -> bool:
        """
        Receive a response from the Electrum server.

        Returns True if a valid response is received, False otherwise.
        """
        response = self.read_response()
        self.log.debug(f"Response: {response}")
        result = json.loads(response)

        # Notifications do not have an `id`. They can arrive before the
        # response we are waiting for, so keep reading until the matching
        # response shows up.
        if result.get("id") != self._request_id:
            if "method" in result and "id" not in result:
                self.log.debug(f"Ignoring notification: {result}")
                return False

            self.log.debug(
                f"Ignoring unmatched message with id {result.get('id')}: {result}"
            )
            return False

        self.result = result
        return True

    def read_response(self) -> str:
        """
        Receive a single JSON-RPC message from the server (delineated by ``\\n``).

        Electrum servers can send notifications asynchronously, so we may need
        to read several messages before we get the response for the current
        request.
        """
        response = b""
        byte = self.conn.recv(BUFFER_SIZE)
        while byte and byte != b"\n":
            response += byte
            byte = self.conn.recv(BUFFER_SIZE)
        return response.decode("utf-8").strip()

    def _next_request_id(self) -> int:
        self._request_id += 1
        return self._request_id

    def request(self, method, params) -> object:
        """
        Request something to Floresta server
        """
        if not self.is_connected:
            self.create_connection()
            if not self.is_connected:
                raise ConnectionError("Could not connect to Electrum server")

        request_id = self._next_request_id()
        request = json.dumps(
            {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
        )

        mnt_point = "/".join(method.split("."))
        self.log.debug(f"GET electrum://{mnt_point}?params={params}")
        self.conn.sendall(request.encode("utf-8") + b"\n")

        # pylint: disable=unnecessary-lambda
        # Lambda is required here because wait_until needs a callable to invoke
        # repeatedly until it returns True. Without it, receive_response() would
        # execute immediately and pass its result, not a function.
        wait_until(lambda: self.receive_response(), interval=0)

        # Check for JSON-RPC error response
        error = self.result.get("error")
        if error is not None:
            raise ValueError(
                f"Electrum RPC error {error.get('code')}: {error.get('message')}"
            )

        # Return only the result, not the whole response
        return self.result.get("result")
