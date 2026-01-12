"""
tests/test_framework/daemon/base.py

Define a base class for a daemon(e.g, florestad and utreexod) process for
that can be started in regtest mode.
"""

import os
import time

from abc import ABC, abstractmethod
from datetime import datetime, timezone
from subprocess import Popen, PIPE
from typing import List
from test_framework.rpc import ConfigRPC
from test_framework.daemon import ConfigP2P
from test_framework.electrum import ConfigElectrum


class BaseDaemon(ABC):
    """
    Base class for managing daemon processes (e.g., florestad, utreexod) in regtest mode.

    This class provides the necessary methods to configure, start, and manage daemon processes.
    It ensures proper initialization, validation, and graceful termination of the processes.
    The class abstracts common functionality for handling RPC, P2P, and Electrum configurations.
    """

    # pylint: disable=too-many-arguments too-many-instance-attributes too-many-positional-arguments
    def __init__(
        self,
        name: str,
        target: str,
        data_dir: str,
        p2p_config: ConfigP2P,
        rpc_config: ConfigRPC,
        extra_args: List[str],
        electrum_config: ConfigElectrum,
    ):
        self._process = None
        self._name: str = name
        self._data_dir: str = data_dir
        self._electrum_config: ConfigElectrum = electrum_config

        if not os.path.exists(target):
            raise ValueError(f"Target path {target} does not exist")
        self._target: str = target
        self._p2p_config: ConfigP2P = p2p_config

        setting: List[str] = self.get_cmd_network()
        if len(extra_args) >= 1:
            setting.extend(extra_args)
        # This configuration needs to be after extra_args
        # to allow overriding default settings
        setting.extend(self.get_cmd_data_dir(data_dir))
        setting.extend(self.get_cmd_rpc(rpc_config))
        setting.extend(self.get_cmd_p2p(p2p_config))
        setting.extend(self.get_cmd_electrum(electrum_config))
        self._settings: List[str] = setting

    # pylint: disable=R0801
    def log(self, message: str):
        """Log a message to the console"""
        now = (
            datetime.now(timezone.utc)
            .replace(microsecond=0)
            .strftime("%Y-%m-%d %H:%M:%S")
        )

        print(f"[{self.__class__.__name__.upper()} {now}] {message}")

    @property
    def target(self) -> str:
        """Getter for `target` property"""
        return self._target

    @property
    def name(self) -> str:
        """Getter for `name` property"""
        return self._name

    @property
    def process(self) -> Popen:
        """Getter for `process` property"""
        return self._process

    @process.setter
    def process(self, value: Popen):
        """Setter for `process` property"""
        self._process = value

    @property
    def is_running(self) -> bool:
        """Check if the daemon process is running"""
        return self.process is not None and self.process.poll() is None

    @property
    def data_dir(self) -> str:
        """Getter for `data_dir` property"""
        return self._data_dir

    @property
    def p2p_url(self) -> str:
        """Getter for `p2p_url` property"""
        return f"{self._p2p_config.host}:{self._p2p_config.port}"

    def start(self):
        """
        Start the daemon process in regtest mode.

        Validates if the process starts successfully; otherwise, logs the error
        and terminates the process.
        """
        if self.is_running:
            raise RuntimeError(f"Daemon '{self.name}' is already running")

        daemon = os.path.normpath(os.path.join(self.target, self.name))
        if not os.path.exists(daemon):
            raise ValueError(f"Daemon path {daemon} does not exist")

        cmd = [daemon] + self._settings

        # pylint: disable=consider-using-with
        self.process = Popen(cmd, text=True, stderr=PIPE)

        # Wait a little to see if the process is running
        time.sleep(1)
        if not self.is_running:
            self.process.terminate()
            stderr = self.process.stderr.read()

            self.log(f"Failed to start node '{self.name}'. Command: {' '.join(cmd)} ")
            raise RuntimeError(f"Failed to start node '{self.name}'. {stderr}")

        self.log(f"Starting node '{self.name}': {' '.join(cmd)}")

    @abstractmethod
    def get_cmd_network(self) -> List[str]:
        """
        Return the network configuration flags for the node.
        """

    @abstractmethod
    def get_cmd_data_dir(self, data_dir: str) -> List[str]:
        """
        Return the data directory configuration flags for the node.
        """

    @abstractmethod
    def get_cmd_rpc(self, config: ConfigRPC) -> List[str]:
        """
        Return the RPC configuration flags for the node.
        """

    @abstractmethod
    def get_cmd_p2p(self, config: ConfigP2P) -> List[str]:
        """
        Return the P2P configuration flags for the node.
        """

    @abstractmethod
    def get_cmd_electrum(self, config: ConfigElectrum) -> List[str]:
        """
        Return the Electrum configuration flags for the node.
        """
