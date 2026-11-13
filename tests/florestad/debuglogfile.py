# SPDX-License-Identifier: MIT OR Apache-2.0

"""
florestad/debuglogfile.py

This functional test tests where florestad writes its log file, covering the
default, the `--no-log-file` opt-out and a custom `--log-file` path. Nodes run
on regtest, so the log lands in the `regtest` subdirectory of the data dir.
"""

import os

import pytest

from test_framework.node import NodeType

# Default log file name, matching Bitcoin Core.
DEFAULT_LOG_FILE = "debug.log"

# Nodes run on regtest, which gets its own subdirectory of the data dir.
NETWORK_SUBDIR = "regtest"


def log_dir(node) -> str:
    """
    Return the network-specific directory florestad logs into.
    """
    return os.path.join(node.daemon.data_dir, NETWORK_SUBDIR)


@pytest.mark.florestad
def test_log_file_written_by_default(florestad_node):
    """
    With no flags, florestad writes to `<data_dir>/regtest/debug.log`.
    """
    log_file = os.path.join(log_dir(florestad_node), DEFAULT_LOG_FILE)
    assert os.path.isfile(log_file), f"expected a log file at {log_file}"
    assert os.path.getsize(log_file) > 0, "log file should not be empty"


@pytest.mark.florestad
def test_no_log_file_disables_file_logging(add_node_with_extra_args):
    """
    `--no-log-file` stops florestad writing any log file.
    """
    florestad = add_node_with_extra_args(
        NodeType.FLORESTAD,
        extra_args=["--no-log-file"],
    )

    directory = log_dir(florestad)
    logs = [name for name in os.listdir(directory) if name.endswith(".log")]
    assert not logs, f"expected no log file, found {logs} in {directory}"


@pytest.mark.florestad
def test_relative_log_file_lands_under_the_data_dir(add_node_with_extra_args):
    """
    A relative `--log-file` is prefixed by the network-specific data dir.
    """
    florestad = add_node_with_extra_args(
        NodeType.FLORESTAD,
        extra_args=["--log-file=custom.log"],
    )

    directory = log_dir(florestad)
    custom = os.path.join(directory, "custom.log")
    assert os.path.isfile(custom), f"expected a log file at {custom}"

    default = os.path.join(directory, DEFAULT_LOG_FILE)
    assert not os.path.isfile(default), "the default log file should not be written too"


@pytest.mark.florestad
def test_core_flag_aliases_are_accepted(add_node_with_extra_args):
    """
    Core's `--debuglogfile` spelling still selects the log file.
    """
    florestad = add_node_with_extra_args(
        NodeType.FLORESTAD,
        extra_args=["--debuglogfile=core-spelling.log"],
    )

    aliased = os.path.join(log_dir(florestad), "core-spelling.log")
    assert os.path.isfile(aliased), f"expected a log file at {aliased}"


@pytest.mark.florestad
def test_core_no_log_file_alias_disables_file_logging(add_node_with_extra_args):
    """
    Core's `--nodebuglogfile` spelling also stops florestad writing a log file.
    """
    florestad = add_node_with_extra_args(
        NodeType.FLORESTAD,
        extra_args=["--nodebuglogfile"],
    )

    directory = log_dir(florestad)
    logs = [name for name in os.listdir(directory) if name.endswith(".log")]
    assert not logs, f"expected no log file, found {logs} in {directory}"
