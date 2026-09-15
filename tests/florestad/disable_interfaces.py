# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Test disabling JSON-RPC and Electrum interface servers in florestad.
"""

import socket
import pytest

from test_framework.node import NodeType
from test_framework.util import wait_until


def is_tcp_listening(host: str, port: int) -> bool:
    """Return True if something is accepting TCP connections on host:port."""
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.settimeout(0.5)
        return sock.connect_ex((host, port)) == 0


@pytest.mark.florestad
def test_disable_rpc(node_manager):
    """
    Test starting florestad with --disable-rpc flag.
    """
    node = node_manager.add_node_extra_args(
        variant=NodeType.FLORESTAD,
        extra_args=["--disable-rpc"],
    )
    node.daemon.start()

    electrum_host = node.daemon.electrum_config.host
    electrum_port = node.daemon.electrum_config.port

    wait_until(
        lambda: is_tcp_listening(electrum_host, electrum_port),
        timeout=30,
        error_msg="Electrum port should be open",
    )

    assert node.rpc.try_wait_on_socket(
        opened=False, timeout=10
    ), "RPC port should be closed when --disable-rpc is set"

    node.stop()


@pytest.mark.florestad
def test_disable_electrum(node_manager):
    """
    Test starting florestad with --disable-electrum flag.
    """
    node = node_manager.add_node_extra_args(
        variant=NodeType.FLORESTAD,
        extra_args=["--disable-electrum"],
    )
    node.daemon.start()

    node.rpc.wait_on_socket(opened=True)

    electrum_host = node.daemon.electrum_config.host
    electrum_port = node.daemon.electrum_config.port

    wait_until(
        lambda: not is_tcp_listening(electrum_host, electrum_port),
        timeout=30,
        error_msg="Electrum port should be closed when --disable-electrum is set",
    )

    node.stop()


@pytest.mark.florestad
@pytest.mark.skip(reason="Requires florestad compiled with --features zmq-server")
def test_disable_zmq(node_manager):
    """
    Test starting florestad with --disable-zmq flag.

    Note: This test requires florestad to be compiled with the 'zmq-server'
    feature flag. Since the default test suite does not enable this feature,
    this test is skipped by default.
    """
    node = node_manager.add_node_extra_args(
        variant=NodeType.FLORESTAD,
        extra_args=["--disable-zmq"],
    )
    node.daemon.start()

    node.rpc.wait_on_socket(opened=True)

    wait_until(
        lambda: not is_tcp_listening("127.0.0.1", 5150),
        timeout=30,
        error_msg="ZMQ port should be closed when --disable-zmq is set",
    )

    node.stop()
