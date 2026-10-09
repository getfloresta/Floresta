# SPDX-License-Identifier: MIT OR Apache-2.0

"""
tests/test_framework/electrum/reference.py

Start romanz/electrs and ElectrumX on a bitcoind node, as references for
Floresta's Electrum server. `tests/prepare.sh --electrum-compat` fetches
electrs; the `electrum-compat` extra in pyproject.toml installs ElectrumX.
"""

import importlib.util
import os
import subprocess
import sys

from test_framework.electrum import ConfigElectrum
from test_framework.electrum.client import ElectrumClient
from test_framework.node import Node
from test_framework.util import Utility, wait_until

# The only protocol version Floresta, electrs and ElectrumX all speak
PROTOCOL = "1.4"


def electrs_path() -> str | None:
    """Path of the electrs binary, or None if prepare.sh did not fetch it."""
    path = os.path.join(Utility.get_integration_test_dir(), "binaries", "electrs")
    return path if os.path.exists(path) else None


def electrumx_installed() -> bool:
    """Whether the `electrum-compat` extra is installed."""
    return importlib.util.find_spec("electrumx") is not None


def connect(port: int, log, protocol=PROTOCOL) -> tuple[ElectrumClient, str]:
    """
    Open a connection and negotiate `protocol`, which must be the first request.
    Return the client and the version the server chose.
    """
    client = ElectrumClient(ConfigElectrum("127.0.0.1", port, None), log)
    _, version = client.get_version(protocol)
    return client, version


# pylint: disable=too-many-instance-attributes
class ReferenceServer:
    """An Electrum server process with a client connected to it."""

    def __init__(self, name: str, test_name: str, log):
        self.name = name
        self.port = Utility.get_random_port()
        self.data_dir = os.path.join(
            Utility.get_integration_test_dir(), "data", test_name, name
        )
        self.log_file = os.path.join(Utility.get_log_path(), test_name, f"{name}.log")
        self.log = log
        self.process = None
        self.client = None
        self.version = None
        os.makedirs(self.data_dir, exist_ok=True)
        os.makedirs(os.path.dirname(self.log_file), exist_ok=True)

    def start(self, cmd: list[str], env: dict | None = None):
        """Start the server and wait until it answers."""
        with open(self.log_file, "w", encoding="utf-8") as out:
            # pylint: disable=consider-using-with
            self.process = subprocess.Popen(
                cmd, env=env, stdout=out, stderr=subprocess.STDOUT
            )

        def answers() -> bool:
            if self.process.poll() is not None:
                raise RuntimeError(f"{self.name} exited, see {self.log_file}")
            try:
                self.client, self.version = connect(self.port, self.log)
            except ConnectionError:
                return False
            return True

        try:
            wait_until(answers, timeout=60, error_msg=f"{self.name} did not answer")
        except Exception:
            self.stop()
            raise

    def wait_for_height(self, height: int):
        """Wait until the server has indexed the chain up to `height`."""
        wait_until(
            lambda: self.client.request("blockchain.headers.subscribe", [])["height"]
            == height,
            error_msg=f"{self.name} did not reach height {height}",
        )

    def stop(self):
        """Stop the server, killing it if it does not exit in time."""
        if self.client is not None and self.client.is_connected:
            self.client.conn.close()
        if self.process is None or self.process.poll() is not None:
            return
        self.process.terminate()
        try:
            self.process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()


def start_electrs(bitcoind: Node, test_name: str, log) -> ReferenceServer:
    """Start electrs on `bitcoind`, which it reaches by RPC and P2P."""
    server = ReferenceServer("electrs", test_name, log)
    rpc = bitcoind.rpc.config

    # electrs takes RPC credentials only from a file, read as is: no newline
    cookie = os.path.join(server.data_dir, "cookie")
    with open(cookie, "w", encoding="utf-8") as f:
        f.write(f"{rpc.user}:{rpc.password}")

    server.start(
        [
            electrs_path(),
            "--skip-default-conf-files",
            "--network=regtest",
            f"--db-dir={server.data_dir}",
            f"--daemon-rpc-addr={rpc.host}:{rpc.port}",
            f"--daemon-p2p-addr={bitcoind.p2p_url}",
            f"--cookie-file={cookie}",
            f"--electrum-rpc-addr=127.0.0.1:{server.port}",
            f"--monitoring-addr=127.0.0.1:{Utility.get_random_port()}",
        ]
    )
    return server


def start_electrumx(bitcoind: Node, test_name: str, log) -> ReferenceServer:
    """Start ElectrumX on `bitcoind`, which needs `-txindex`."""
    server = ReferenceServer("electrumx", test_name, log)
    rpc = bitcoind.rpc.config

    server.start(
        [sys.executable, "-m", "electrumx.cli.electrumx_server"],
        env={
            **os.environ,
            "COIN": "Bitcoin",
            "NET": "regtest",
            "DAEMON_URL": f"http://{rpc.user}:{rpc.password}@{rpc.host}:{rpc.port}/",
            "DB_DIRECTORY": server.data_dir,
            "SERVICES": f"tcp://127.0.0.1:{server.port}",
            "PEER_DISCOVERY": "off",
            # Poll bitcoind every 0.5 s, not 5 s
            "DAEMON_POLL_INTERVAL_BLOCKS": "500",
        },
    )
    return server
