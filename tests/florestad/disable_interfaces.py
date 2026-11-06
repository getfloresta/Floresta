# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Test disabling the JSON-RPC, Electrum and ZMQ interface servers in florestad.

Each test starts the daemon directly, instead of going through
`node_manager.run_node`, because that helper waits on the JSON-RPC socket and
one of the interfaces under test is always down. To avoid asserting on a port
that is merely not bound *yet*, every test first waits for an interface that is
still enabled, and only then checks that the disabled one refuses connections.
"""

import pytest

from test_framework.node import NodeType
from test_framework.util import Utility, is_port_open, wait_until


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


@pytest.mark.florestad
@pytest.mark.skip(
    reason="needs florestad built with --features zmq-server, which "
    "tests/prepare.sh does not enable"
)
def test_disable_zmq(node_manager):
    """
    Test starting florestad with --disable-zmq flag.

    The ZMQ server only exists in builds that enable the 'zmq-server' feature,
    so this test is skipped by default (see the skip reason above).
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
