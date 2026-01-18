"""
tests/test_framework/rpc/base.py

Define a base class for making RPC calls to a
`test_framework.daemon.floresta.BaseDaemon`.
"""

import json
import socket
import time
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional
from urllib.parse import quote
from abc import ABC, abstractmethod

from requests import post
from requests.exceptions import HTTPError
from requests.models import HTTPBasicAuth
from test_framework.rpc.exceptions import JSONRPCError
from test_framework.rpc import ConfigRPC


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
        self._user: Optional[str] = config.user
        self._password: Optional[str] = config.password
        self._host: str = config.host
        self._port: int = config.port
        self._address: str = f"http://{self._host}:{self._port}"
        self._jsonrpc_version: str = self.get_jsonrpc_version()

    @abstractmethod
    def get_jsonrpc_version(self) -> str:
        """Get the JSON-RPC version used by this RPC connection."""

    @property
    def address(self) -> str:
        """Get the RPC server address."""
        return self._address

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
        if self._user is not None and self._password is not None:
            request["auth"] = HTTPBasicAuth(self._user, self._password)

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
            request["url"], method, params, self._user, self._password
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

    def is_connection_open(self) -> bool:
        """Returns True if a TCP port is open (connection succeeded)."""
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(0.5)
            connected = sock.connect_ex((self._host, self._port))
            return connected == 0

    def try_wait_for_connection(self, opened: bool, timeout: float) -> bool:
        """
        Wait for the RPC connection to reach the desired state within a timeout.
        Returns True if successful, otherwise False.
        """
        start = time.time()
        while time.time() - start < timeout:
            if self.is_connection_open() == opened:
                state = "open" if opened else "closed"
                self.log(f"{self._host}:{self._port} {state}")
                return True
            time.sleep(0.5)

        return False

    def wait_for_connection(self, opened: bool):
        """
        Ensure the RPC connection reaches the desired state within a timeout.
        Raises TimeoutError if the state is not reached.
        """
        timeout = self.TIMEOUT
        success = self.try_wait_for_connection(opened, timeout)
        if not success:
            state = "open" if opened else "closed"
            raise TimeoutError(f"{self.address} not {state} after {timeout} seconds")

    def get_blockchain_info(self) -> dict:
        """
        Get the blockchain info by performing `perform_request('getblockchaininfo')`
        """
        return self.perform_request("getblockchaininfo")

    def stop(self):
        """
        Perform the `stop` RPC command to the daemon and wait for the connection to close
        """
        result = self.perform_request("stop")
        self.wait_for_connection(opened=False)
        return result

    # pylint: disable=R0801
    def get_bestblockhash(self) -> str:
        """
        Get the hash of the best block in the chain performing
        `perform_request('getbestblockhash')`
        """
        return self.perform_request("getbestblockhash")

    def get_blockhash(self, height: int) -> str:
        """
        Get the blockhash associated with a given height performing
        `perform_request('getblockhash', params=[<int>])`
        """
        return self.perform_request("getblockhash", [height])

    def get_block_count(self) -> int:
        """
        Get block count of the node by performing `perform_request('getblockcount')`
        """
        return self.perform_request("getblockcount")

    def get_peerinfo(self):
        """
        Get the peer information by performing `perform_request('getpeerinfo')`
        """
        return self.perform_request("getpeerinfo")

    def get_rpcinfo(self):
        """
        Returns stats about our RPC server performing
        `perform_request('getrpcinfo')`
        """
        return self.perform_request("getrpcinfo")

    def uptime(self) -> int:
        """
        Get the uptime of the node by performing `perform_request('uptime')`
        """
        return self.perform_request("uptime")

    def get_txout(self, txid: str, vout: int, include_mempool: bool) -> dict:
        """
        Get transaction output by performing `perform_request('gettxout', params=[str, int])`

        Args:
            txid: The transaction ID
            vout: The output index

        Returns:
            The transaction output information
        """
        return self.perform_request("gettxout", params=[txid, vout, include_mempool])

    def ping(self):
        """
        Tells our node to send a ping to all its peers
        """
        return self.perform_request("ping")
