"""
tests/test_framework/rpc/base.py

Define a base class for making RPC calls to a
`test_framework.daemon.floresta.BaseDaemon`.
"""

import json
import socket
import time
import re
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from urllib.parse import quote
from abc import ABC, abstractmethod

from requests import post
from requests.exceptions import HTTPError
from requests.models import HTTPBasicAuth
from test_framework.rpc.exceptions import JSONRPCError
from test_framework.rpc import ConfigRPC


# pylint: disable=too-many-public-methods
class BaseRPC(ABC):
    """
    Abstract base class for managing JSON-RPC connections to a daemon.

    This class defines the structure and common functionality for RPC clients.
    Subclasses must implement specific RPC methods (e.g., `get_jsonrpc_version`)
    and define the JSON-RPC version used.

    Responsibilities:
    - Establish and manage the RPC connection.
    - Provide utility methods for building and sending RPC requests.
    - Handle connection state and errors.

    Subclasses should use `perform_request` to implement RPC calls.
    """

    TIMEOUT: int = 15  # seconds

    def __init__(self, config: ConfigRPC):
        self._config = config
        self._jsonrpc_version: str = self.get_jsonrpc_version()

    @property
    def config(self) -> ConfigRPC:
        """Getter for `config` property"""
        return self._config

    def set_config(self, value: ConfigRPC):
        """Setter for `config` property"""
        self._config = value

    @property
    def address(self) -> str:
        """Get the RPC server address."""
        return f"http://{self.config.host}:{self.config.port}"

    @abstractmethod
    def get_jsonrpc_version(self) -> str:
        """Get the JSON-RPC version used by this RPC connection."""

    # pylint: disable=R0801
    def log(self, message: str):
        """Log a message to the console"""
        now = (
            datetime.now(timezone.utc)
            .replace(microsecond=0)
            .strftime("%Y-%m-%d %H:%M:%S")
        )

        print(f"[{self.__class__.__name__.upper()} {now}] {message}")

    @staticmethod
    def build_log_message(
        url: str,
        method: str,
        params: List[Any],
        user: Optional[str] = None,
        password: Optional[str] = None,
    ) -> str:
        """
        Construct a log string for an RPC call like:
        POST <user:password>@http://host:port/method?args[0]=val0&args[1]=val1...
        """
        logmsg = "POST "

        if user or password:
            logmsg += f"<{user or ''}:{password or ''}>@"

        logmsg += f"{url}/{method}"

        if params:
            query_string = "&".join(
                f"args[{i}]={quote(str(val))}" for i, val in enumerate(params)
            )
            logmsg += f"?{query_string}"

        return logmsg

    def build_request(self, method: str, params: List[Any]) -> Dict[str, Any]:
        """
        Build the request dictionary for the RPC call.
        """
        request = {
            "url": f"{self.address}",
            "headers": {"content-type": "application/json"},
            "data": json.dumps(
                {
                    "jsonrpc": self._jsonrpc_version,
                    "id": "0",
                    "method": method,
                    "params": params,
                }
            ),
            "timeout": self.TIMEOUT,
        }
        if self._config.user is not None and self._config.password is not None:
            request["auth"] = HTTPBasicAuth(self._config.user, self._config.password)

        return request

    # pylint: disable=unused-argument,dangerous-default-value
    def perform_request(
        self,
        method: str,
        params: List[int | str | float | Dict[str, str | Dict[str, str]]] = [],
    ) -> Any:
        """
        Perform a JSON-RPC request to the RPC server given the method
        and params. The params should be a list of arguments to the
        method. The method should be a string with the name of the
        method to be called.

        The method will return the result of the request or raise
        a JSONRPCError if the request failed.
        """
        request = self.build_request(method, params)

        # Now make the POST request to the RPC server
        logmsg = BaseRPC.build_log_message(
            request["url"], method, params, self._config.user, self._config.password
        )

        self.log(logmsg)
        response = post(**request)

        # If response isnt 200, raise an HTTPError
        if response.status_code != 200:
            raise HTTPError

        result = response.json()
        # Error could be None or a str
        # If in the future this change,
        # cast the resulted error to str
        if "error" in result and result["error"] is not None:
            raise JSONRPCError(
                data=result["error"] if isinstance(result["error"], str) else None,
                rpc_id=result["id"],
                code=result["error"]["code"],
                message=result["error"]["message"],
            )

        self.log(result["result"])
        return result["result"]

    def is_socket_listening(self) -> bool:
        """Check if the socket is listening for connections on the specified port."""
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(0.5)
            connected = sock.connect_ex((self._config.host, self._config.port))
            return connected == 0

    def try_wait_on_socket(self, opened: bool, timeout: float) -> bool:
        """
        Verifies that the RPC socket connection matches the expected one, retrying for
        the specified duration. Returns true if the connection matches, false otherwise.
        """
        start = time.time()
        while time.time() - start < timeout:
            if self.is_socket_listening() == opened:
                state = "open" if opened else "closed"
                self.log(f"{self._config.host}:{self._config.port} {state}")
                return True
            time.sleep(0.5)

        return False

    def wait_on_socket(self, opened: bool):
        """
        Ensure the RPC connection reaches the desired state within a timeout.
        Raises TimeoutError if the state is not reached.
        """
        timeout = self.TIMEOUT
        success = self.try_wait_on_socket(opened, timeout)
        if not success:
            state = "open" if opened else "closed"
            raise TimeoutError(f"{self.address} not {state} after {timeout} seconds")

    def get_blockchain_info(self) -> dict:
        """
        Get the blockchain info
        """
        return self.perform_request("getblockchaininfo")

    def stop(self):
        """
        Perform the `stop` RPC command to the daemon and wait for the connection to close
        """
        result = self.perform_request("stop")
        self.wait_on_socket(opened=False)
        return result

    def addnode(self, node: str, command: str, v2transport: bool = False):
        """
        Adds a new node to our list of peers performing

        This will make our node try to connect to this peer.
        """
        # matches, IPv4, IPv6 and optional ports from 0 to 65535
        pattern = re.compile(
            r"^("
            r"(?:(?:25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])\.){3}"
            r"(?:25[0-5]|2[0-4][0-9]|1[0-9]{2}|[1-9]?[0-9])|"
            r"\[([a-fA-F0-9:]+)\]"
            r")"
            r"(:(6553[0-5]|655[0-2][0-9]|65[0-4][0-9]{2}|6[0-4][0-9]{3}|[1-9]?[0-9]{1,4}))?$"
        )

        if not pattern.match(node):
            raise ValueError("Invalid ip[:port] format")

        if command not in ("add", "remove", "onetry"):
            raise ValueError(f"Invalid command '{command}'")

        return self.perform_request("addnode", params=[node, command, v2transport])

    # pylint: disable=R0801
    def get_bestblockhash(self) -> str:
        """
        Get the hash of the best block in the chain
        """
        return self.perform_request("getbestblockhash")

    def get_blockhash(self, height: int) -> str:
        """
        Get the blockhash associated with a given height
        """
        return self.perform_request("getblockhash", [height])

    def get_block_count(self) -> int:
        """
        Get block count of the node
        """
        return self.perform_request("getblockcount")

    def get_blockheader(self, blockhash: str, verbose: bool = True) -> dict:
        """
        Get the header of a block.

        When `verbose` is True (the default), returns a dict with block header
        fields. When False, returns the hex-encoded serialized block header.
        """
        if not bool(re.fullmatch(r"^[a-f0-9]{64}$", blockhash)):
            raise ValueError(f"Invalid blockhash '{blockhash}'.")

        return self.perform_request("getblockheader", params=[blockhash, verbose])

    def get_block(self, blockhash: str, verbosity: int = 1):
        """
        Get a full block, given its hash performing

        Notice that this rpc will cause a actual network request to our node,
        so it may be slow, and if used too often, may cause more network usage.
        The returns for this rpc are identical to bitcoin core's getblock rpc
        as of version 27.0.

        the `str` param should be a valid 32 bytes hex formatted string
        the `int` param should be a integer verbosity level
        """
        if len(blockhash) != 64:
            raise ValueError(f"invalid blockhash param: {blockhash}")

        if verbosity not in (0, 1):
            raise ValueError(f"Invalid verbosity level param: {verbosity}")

        return self.perform_request("getblock", params=[blockhash, verbosity])

    def get_peerinfo(self):
        """
        Get the peer information
        """
        return self.perform_request("getpeerinfo")

    def get_rpcinfo(self):
        """
        Returns stats about our RPC server
        """
        return self.perform_request("getrpcinfo")

    def uptime(self) -> int:
        """
        Get the uptime of the node
        """
        return self.perform_request("uptime")

    def get_txout(self, txid: str, vout: int, include_mempool: bool) -> dict:
        """
        Get transaction output
        """
        return self.perform_request("gettxout", params=[txid, vout, include_mempool])

    def ping(self):
        """
        Tells our node to send a ping to all its peers
        """
        return self.perform_request("ping")

    def disconnectnode(self, node_address: str, node_id: Optional[int] = None):
        """
        Disconnect from a peer by `node_address` or `node_id`
        """

        if node_id is not None:
            return self.perform_request(
                "disconnectnode", params=[node_address, node_id]
            )
        return self.perform_request("disconnectnode", params=[node_address])
