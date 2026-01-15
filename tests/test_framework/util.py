"""Utility helpers used by the test framework (paths, ports, TLS helpers)."""

import os
import random
import socket
import subprocess

from test_framework.crypto.pkcs8 import (
    create_pkcs8_private_key,
    create_pkcs8_self_signed_certificate,
)


class Utility:
    """
    A utility class for common functions used in the test framework.
    """

    @staticmethod
    def get_integration_test_dir():
        """
        Get path for florestad used in integration tests, generally set on
        $FLORESTA_TEMP_DIR/binaries
        """
        if os.getenv("FLORESTA_TEMP_DIR") is None:
            raise RuntimeError(
                "FLORESTA_TEMP_DIR not set. "
                + " Please set it to the path of the integration test directory."
            )
        return os.getenv("FLORESTA_TEMP_DIR")

    @staticmethod
    def get_git_describe():
        """
        Get the output of 'git describe --tags --always' command.
        """
        try:
            git_describe = subprocess.check_output(
                ["git", "describe", "--tags", "--always"], text=True
            ).strip()
        except subprocess.CalledProcessError as exc:
            raise RuntimeError(
                "Failed to run 'git describe'. Run this at the Floresta directory."
            ) from exc

        return git_describe

    @staticmethod
    def get_available_random_port_by_range(start: int, end: int):
        """Get an available random port in the range [start, end]"""
        while True:
            port = random.randint(start, end)
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                # Check if the port is available
                if s.connect_ex(("127.0.0.1", port)) != 0:
                    return port

    @staticmethod
    def get_random_port():
        """Get a random port in the range [2000, 65535]"""
        return Utility.get_available_random_port_by_range(2000, 65535)

    @staticmethod
    def create_tls_key_cert() -> tuple[str, str]:
        """
        Create a PKCS#8 formatted private key and a self-signed certificate.
        These keys are intended to be used with florestad's --tls-key-path and --tls-cert-path
        options.
        """
        # If we're in CI, we need to use the
        # path to the integration test dir
        # tempfile will be used to get the proper
        # temp dir for the OS
        tls_rel_path = os.path.join(Utility.get_integration_test_dir(), "data", "tls")
        tls_path = os.path.normpath(os.path.abspath(tls_rel_path))

        # Create the folder if not exists
        os.makedirs(tls_path, exist_ok=True)

        # Create certificates
        pk_path, private_key = create_pkcs8_private_key(tls_path)

        cert_path = create_pkcs8_self_signed_certificate(
            tls_path, private_key, common_name="florestad", validity_days=365
        )

        return (pk_path, cert_path)
