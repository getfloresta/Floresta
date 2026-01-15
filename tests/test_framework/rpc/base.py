"""
tests/test_framework/rpc/base.py

Define a base class for making RPC calls to a
`test_framework.daemon.floresta.BaseDaemon`.
"""

import json
import socket
import time
from subprocess import Popen
from typing import Any, Dict, List, Optional
from urllib.parse import quote

from requests import post
from requests.exceptions import HTTPError
from requests.models import HTTPBasicAuth
from test_framework.rpc.exceptions import JSONRPCError


class BaseRpcMetaClass(type):
    """
    Metaclass for BaseDaemon.

    Ensures that any attempt to register a subclass of `BaseDaemon` will
    adheres to a standard whereby the subclass DOES NOT override either
    `__init__`, `log`, `perform_request`, `is_connection_open`,
    `wait_for_connection`, `wait_for_connections`.  But must
    implement `get_blockchain_info` and `stop`.

    If any of those standards are violated, a ``TypeError`` is raised.
    """

    # pylint: disable=too-many-boolean-expressions
    def __new__(mcs, clsname, bases, dct):
        if not clsname == "BaseRPC":

            if "get_blockchain_info" not in dct or "stop" not in dct:
                raise TypeError(
                    "BaseRPC subclasses must override 'get_blockchain_info' and 'stop'"
                )
            # Here we want just that developer do not implement
            # those methods, since we desire to have a standard
            # behavior for all RPC classes.
            if any(
                method in dct
                for method in (
                    "__init__",
                    "log",
                    "perform_request",
                    "is_connection_open",
                    "wait_for_connection",
                    "wait_for_connections",
                )
            ):
                raise TypeError(
                    "BaseRPC subclasses must not override  '__init__', 'log',"
                    + "'perform_request', 'is_connection_open', "
                    + "'wait_for_connection', 'wait_for_connections', "
                    + "'get_blockchain_info' and 'stop'"
                )

        return super().__new__(mcs, clsname, bases, dct)


# pylint: disable=too-few-public-methods
class RPCServerConfig:
    """
    A class for storing the RPC server configuration.
    """

    def __init__(self, **kwargs):
        self.host = kwargs.get("host", "127.0.0.1")
        self.ports = kwargs.get("ports", {})
        self.user = kwargs.get("user", None)
        self.password = kwargs.get("password", None)
        self.jsonrpc_version = kwargs.get("jsonrpc", "1.0")
        self.timeout = kwargs.get("timeout", 10000)


class BaseRPC(metaclass=BaseRpcMetaClass):
    """
    A class for making RPC calls to a `test_framework.daemon.floresta.BaseDaemon`.

    The instantiation of a superclass of `BaseRPC` will create a new
    RPC connection to the daemon process. The RPC connection is
    established using the `requests` library, using the method
    `perform_request`.

    Every desired RPC call would be implemented using that method.
    Two of them are imperative, `get_blockchain_info`, `stop`. For example:

    ```python
    class MyRPC(BaseRPC):

        def get_blockchain_info(self) -> dict:
            # you can do any stuff before or after
            # but you must implement it
            return self.perform_request("getblockchaininfo")

        def stop(self):
            # you can do any stuff before or after
            # but you must implement it and we recommend
            # to use `wait_for_connections(opened=False)` after use
            # `perform_request("stop")`
            msg = self.perform_request("stop")
            self.wait_for_connections(opened=False)
            return msg

        def get_stuff(self, stuff: str) -> dict:
            return self.perform_request("getstuff", [stuff])
    ```

    mydaemon = MyDaemon()
    mydaemon.add_daemon_settings(["--mydaemon-arg=value"])
    mydaemon.start()

    MY_RPC_SERVER = {
        "host": "127.0.0.1",
        "ports": {"rpc": 18442, "server": 50001},
        "jsonrpc": "2.0",
        "timeout": 10000,
    }

    myrpc = MyRPC(mydaemon.process, MY_RPC_SERVER)
    info = myrpc.get_blockchain_info()
    stuff = myrpc.get_stuff("stuff")

    # this will send a stop JSON-RPC request to the daemon
    # and stop the daemon process through the RPC connection.
    stop_msg = myrpc.stop()
    """

    def __init__(
        self, log, process: Popen[str], rpcserver: Dict[str, str | Dict[str, str]]
    ):
        self.log = log
        self._rpcserver = RPCServerConfig(**rpcserver)
        self._process = process
        self._rpcconn = None
        self._is_running = False

    @property
    def rpcconn(self):
        """Getter for `rpcconn` property"""
        return self._rpcconn

    @rpcconn.setter
    def rpcconn(self, value: dict):
        """Setter for `rpcconn` property"""
        self._rpcconn = value

    @property
    def process(self) -> Popen[str] | None:
        """Getter for `process` property"""
        return self._process

    @process.setter
    def process(self, value: Popen[str]):
        """Setter for `process` property"""
        self._process = value

    @property
    def rpcserver(self) -> RPCServerConfig:
        """Getter for `rpcsserver` property"""
        return self._rpcserver

    @rpcserver.setter
    def rpcserver(self, value: RPCServerConfig):
        """Setter for `rpcserver` property"""
        self._rpcserver = value

    # pylint: disable=R0801
    def log_msg(self, message: str):
        """Format a log message for the console"""
        return f"[{self.__class__.__name__.upper()}] {message}"

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

        logmsg += f"{url}{method}"

        if params:
            query_string = "&".join(
                f"args[{i}]={quote(str(val))}" for i, val in enumerate(params)
            )
            logmsg += f"?{query_string}"

        return logmsg

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
        # Create basic information for the requests
        # inside the `kwargs` dictionary
        # - url
        # - headers
        # - data (payload)
        # - timeout
        host = getattr(self.rpcserver, "host")
        ports = getattr(self.rpcserver, "ports")
        rpc_port = ports["rpc"]
        user = getattr(self.rpcserver, "user")
        password = getattr(self.rpcserver, "password")
        jsonrpc_version = getattr(self.rpcserver, "jsonrpc_version")
        timeout = getattr(self.rpcserver, "timeout")
        kwargs = {
            "url": f"http://{host}:{rpc_port}/",
            "headers": {"content-type": "application/json"},
            "data": json.dumps(
                {
                    "jsonrpc": jsonrpc_version,
                    "id": "0",
                    "method": method,
                    "params": params,
                }
            ),
            "timeout": timeout,
        }

        # Check if the RPC server has a username and password
        # and set the auth accordingly to HTTPBasicAuth.
        logmsg = "GET "
        if user is not None or password is not None:
            logmsg += f"<{user}:{password}>@"
            kwargs["auth"] = HTTPBasicAuth(user, password)

        # Now make the POST request to the RPC server
        logmsg = BaseRPC.build_log_message(
            kwargs["url"], method, params, user, password
        )

        self.log.debug(self.log_msg(logmsg))
        response = post(**kwargs)

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

        self.log.debug(self.log_msg(result["result"]))
        return result["result"]

    def is_connection_open(self, host: str, port: int) -> bool:
        """Returns True if a TCP port is open (connection succeeded)."""
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            sock.settimeout(0.5)
            connected = sock.connect_ex((host, port))
            return connected == 0

    def wait_for_connection(
        self, host: str, port: int, opened: bool, timeout: int = 180
    ):
        """
        Wait for the RPC connection to reach the desired state.
        If the connection does not reach the desired state in time,
        raise a TimeoutError.
        """
        start = time.time()
        while time.time() - start < timeout:
            if self.is_connection_open(host, port) == opened:
                state = "open" if opened else "closed"
                self.log.debug(self.log_msg(f"{host}:{port} {state}"))
                return
            time.sleep(0.5)

        state = "open" if opened else "closed"
        raise TimeoutError(
            self.log_msg(f"{host}:{port} not {state} after {timeout} seconds")
        )

    def wait_for_connections(self, opened: bool = True, timeout: int = 180):
        """Wait for all port connections in the host reach the desired state."""
        host = getattr(self.rpcserver, "host")
        for _, port in getattr(self.rpcserver, "ports").items():
            self.log.debug(
                self.log_msg(
                    f"Waiting for {host}:{port} to be {'open' if opened else 'closed'}"
                )
            )
            self.wait_for_connection(host, port, opened, timeout)
