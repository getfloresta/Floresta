"""
test_framework.daemon.utreexo.py

A test framework for testing utreexod daemon in regtest mode.
"""

from typing import List

from test_framework.daemon.base import BaseDaemon
from test_framework.rpc import ConfigRPC
from test_framework.daemon import ConfigP2P
from test_framework.electrum import ConfigElectrum


class UtreexoDaemon(BaseDaemon):
    """
    Spawn a new utreexod process on background and run it on
    regtest mode for tests. You can use it to generate blocks
    and utreexo proofs for tests.
    """

    def get_cmd_network(self) -> List[str]:
        """
        Return the network configuration flags for the node.
        """
        return [
            "--regtest",
        ]

    def get_cmd_data_dir(self, data_dir: str) -> List[str]:
        """
        Return the data directory configuration flags for the node.
        """
        return [f"--datadir={data_dir}"]

    def get_cmd_rpc(self, config: ConfigRPC) -> List[str]:
        """
        Return the RPC configuration flags for the node.
        """
        if config.user is None or config.password is None:
            raise ValueError("RPC user and password must be set for utreexod")
        address = f"{config.host}:{config.port}"
        return [
            f"--rpcuser={config.user}",
            f"--rpcpass={config.password}",
            f"--rpclisten={address}",
            "--utreexoproofindex",
        ]

    def get_cmd_p2p(self, config: ConfigP2P) -> List[str]:
        """
        Return the P2P configuration flags for the node.
        """
        address = f"{config.host}:{config.port}"
        return [f"--listen={address}"]

    def get_cmd_electrum(self, config: ConfigElectrum) -> List[str]:
        """
        Return the Electrum configuration flags for the node.
        """
        electrum_settings = []
        electrum_settings.append(f"--electrumlisteners={config.host}:{config.port}")
        if config.tls:
            electrum_settings.extend(
                [
                    f"--rpckey={config.tls.key_file}",
                    f"--rpccert={config.tls.cert_file}",
                    f"--tlselectrumlisteners={config.tls.port}",
                ]
            )
        else:
            electrum_settings.append("--notls")

        return electrum_settings
