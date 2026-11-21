# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Test disabling the JSON-RPC, Electrum and ZMQ interface servers in florestad.

Each test starts the daemon directly, instead of going through
`node_manager.run_node`, because that helper waits on the JSON-RPC socket and
one of the interfaces under test is always down. To avoid asserting on a port
that is merely not bound *yet*, every test first waits for an interface that is
still enabled, and only then checks that the disabled one refuses connections.

A disabled interface must also be visible in the log, both as an `INFO` line
saying the interface is off and as one `WARN` line per flag that was passed but
no longer has any effect, so every test asserts on those lines too.
"""

import os

import pytest

from test_framework.node import NodeType
from test_framework.util import (
    Utility,
    florestad_has_feature,
    is_port_open,
    wait_until,
)

# Nodes run on regtest, which gets its own subdirectory of the data dir, and
# florestad logs to `debug.log` inside it by default.
NETWORK_SUBDIR = "regtest"
DEFAULT_LOG_FILE = "debug.log"


def debug_log_path(node) -> str:
    """
    Return the path of the log file written by a regtest node.
    """
    return os.path.join(node.daemon.data_dir, NETWORK_SUBDIR, DEFAULT_LOG_FILE)


def read_debug_log(node) -> str:
    """
    Return the current contents of the node's log file.
    """
    try:
        with open(debug_log_path(node), encoding="utf-8") as log_file:
            return log_file.read()
    except FileNotFoundError:
        return ""


def wait_for_log_line(node, line: str):
    """
    Wait until `line` shows up in the node's log file.

    Logs reach the file through a non-blocking writer, so a line may land a
    moment after the interface it talks about was started or skipped.
    """
    wait_until(
        lambda: line in read_debug_log(node),
        error_msg=f"expected a log line containing {line!r}",
    )


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

    electrum_config = node.daemon.electrum_config
    rpc_config = node.rpc.config

    # The Electrum server is started after the JSON-RPC one, so once it accepts
    # connections the JSON-RPC server would already have been started too.
    wait_until(
        lambda: is_port_open(electrum_config.host, electrum_config.port),
        error_msg="Electrum port should be open",
    )

    assert not is_port_open(
        rpc_config.host, rpc_config.port
    ), "RPC port should be closed when --disable-rpc is set"

    # The daemon was started with an --rpc-address too, which is now ignored.
    wait_for_log_line(
        node,
        "INFO node: JSON-RPC server is disabled, clients will not be able to connect",
    )
    wait_for_log_line(
        node,
        f"WARN node: --rpc-address {rpc_config.host}:{rpc_config.port} is set but "
        "ignored, the JSON-RPC server is disabled",
    )


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

    electrum_config = node.daemon.electrum_config

    node.rpc.wait_on_socket(opened=True)

    assert not is_port_open(
        electrum_config.host, electrum_config.port
    ), "Electrum port should be closed when --disable-electrum is set"

    # The daemon was started with an --electrum-address too, which is now ignored.
    wait_for_log_line(
        node,
        "INFO node: Electrum server is disabled, the watch-only wallet will not update "
        "for new blocks",
    )
    wait_for_log_line(
        node,
        f"WARN node: --electrum-address {electrum_config.host}:{electrum_config.port} "
        "is set but ignored, the Electrum server is disabled",
    )


@pytest.mark.florestad
@pytest.mark.skipif(
    not florestad_has_feature("--disable-zmq"),
    reason="florestad was not built with the 'zmq-server' feature; rebuild with "
    "`cargo build -p florestad --features zmq-server` to run this test",
)
def test_disable_zmq(node_manager):
    """
    Test starting florestad with --disable-zmq flag.

    The ZMQ server, and with it the --disable-zmq flag, only exists in builds that
    enable the 'zmq-server' feature, so this test runs against such a build and is
    skipped otherwise.
    """
    zmq_host = "127.0.0.1"
    zmq_port = Utility.get_random_port()

    node = node_manager.add_node_extra_args(
        variant=NodeType.FLORESTAD,
        extra_args=["--disable-zmq", f"--zmq-address=tcp://{zmq_host}:{zmq_port}"],
    )
    node.daemon.start()

    node.rpc.wait_on_socket(opened=True)

    assert not is_port_open(
        zmq_host, zmq_port
    ), "ZMQ port should be closed when --disable-zmq is set"

    # The daemon was started with a --zmq-address too, which is now ignored.
    wait_for_log_line(
        node,
        "INFO node: ZMQ server is disabled, new blocks will not be published to clients",
    )
    wait_for_log_line(
        node,
        f"WARN node: --zmq-address tcp://{zmq_host}:{zmq_port} is set but ignored, "
        "the ZMQ server is disabled",
    )


@pytest.mark.florestad
def test_disable_electrum_warns_about_every_ignored_flag(node_manager):
    """
    Every Electrum flag passed alongside --disable-electrum is reported as ignored.
    """
    electrum_tls_port = Utility.get_random_port()

    node = node_manager.add_node_extra_args(
        variant=NodeType.FLORESTAD,
        extra_args=[
            "--disable-electrum",
            "--enable-electrum-tls",
            f"--electrum-address-tls=127.0.0.1:{electrum_tls_port}",
            "--generate-cert",
            "--tls-key-path=key.pem",
            "--tls-cert-path=cert.pem",
        ],
    )
    node.daemon.start()

    node.rpc.wait_on_socket(opened=True)

    electrum_config = node.daemon.electrum_config
    ignored_flags = [
        # --electrum-address is set by the test framework for every node.
        f"--electrum-address {electrum_config.host}:{electrum_config.port}",
        f"--electrum-address-tls 127.0.0.1:{electrum_tls_port}",
        "--enable-electrum-tls",
        "--generate-cert",
        "--tls-key-path key.pem",
        "--tls-cert-path cert.pem",
    ]

    for flag in ignored_flags:
        wait_for_log_line(
            node,
            f"WARN node: {flag} is set but ignored, the Electrum server is disabled",
        )
