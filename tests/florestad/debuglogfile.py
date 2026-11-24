# SPDX-License-Identifier: MIT OR Apache-2.0

"""
florestad/debuglogfile.py

This functional test tests where florestad writes its log file, covering the
default, the `--no-log-file` opt-out and a custom `--log-file` path. Nodes run
on regtest, so the log lands in the `regtest` subdirectory of the data dir.

It also covers trimming an oversized log on startup, and the two cases that
skip it: `--debug` and the `--no-shrink-log-file` opt-out.
"""

import os

import pytest

from test_framework.node import NodeType

# Default log file name, matching Bitcoin Core.
DEFAULT_LOG_FILE = "debug.log"

# Nodes run on regtest, which gets its own subdirectory of the data dir.
NETWORK_SUBDIR = "regtest"


# On startup, a log over 11MB is cut down to its last 10MB, matching Core.
TRIM_THRESHOLD = 11_000_000

# Seeded logs are past the threshold, so a default start trims them.
SEEDED_LOG_SIZE = 12_000_000

# Every seeded line starts with this, so a trimmed log can be checked to
# start on a whole line.
SEEDED_LINE_PREFIX = "2026-01-01 00:00:00  INFO seeded: line "


def log_dir(node) -> str:
    """
    Return the network-specific directory florestad logs into.
    """
    return os.path.join(node.daemon.data_dir, NETWORK_SUBDIR)


def start_with_seeded_log(node_manager, extra_args: list) -> tuple:
    """
    Create a node, write an oversized `debug.log` into its data dir, then start
    it. Returns the log path and the seeded lines.
    """
    node = node_manager.add_node_extra_args(
        variant=NodeType.FLORESTAD,
        extra_args=extra_args,
    )

    directory = log_dir(node)
    os.makedirs(directory, exist_ok=True)

    line_len = len(f"{SEEDED_LINE_PREFIX}0000000\n")
    lines = [
        f"{SEEDED_LINE_PREFIX}{i:07}\n" for i in range(SEEDED_LOG_SIZE // line_len + 1)
    ]
    log_file = os.path.join(directory, DEFAULT_LOG_FILE)
    with open(log_file, "w", encoding="utf-8") as f:
        f.writelines(lines)

    node_manager.run_node(node)
    return log_file, lines


def read_lines(path: str) -> list:
    """
    Return the lines of the file at `path`, keeping their line breaks.
    """
    with open(path, encoding="utf-8") as f:
        return f.readlines()


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


@pytest.mark.florestad
def test_oversized_log_is_trimmed_on_startup(node_manager):
    """
    With no flags, a log over the threshold is cut down to its most recent
    lines, starting on a whole line.
    """
    log_file, seeded = start_with_seeded_log(node_manager, extra_args=[])

    size = os.path.getsize(log_file)
    assert size < TRIM_THRESHOLD, f"expected the log to be trimmed, got {size} bytes"

    kept = read_lines(log_file)
    assert kept[0].startswith(SEEDED_LINE_PREFIX), "trimmed log starts mid-line"
    assert seeded[-1] in kept, "the most recent seeded line should be kept"
    assert seeded[0] not in kept, "the oldest seeded line should be dropped"


@pytest.mark.florestad
def test_debug_keeps_the_whole_log(node_manager):
    """
    `--debug` skips the trim, so the full history is kept while debugging.
    """
    log_file, seeded = start_with_seeded_log(node_manager, extra_args=["--debug"])

    assert os.path.getsize(log_file) >= SEEDED_LOG_SIZE
    assert read_lines(log_file)[0] == seeded[0], "the log should not be trimmed"


@pytest.mark.florestad
@pytest.mark.parametrize("flag", ["--no-shrink-log-file", "--noshrinkdebugfile"])
def test_opt_out_keeps_the_whole_log(node_manager, flag):
    """
    `--no-shrink-log-file`, or Core's `--noshrinkdebugfile`, turns the trim off.
    """
    log_file, seeded = start_with_seeded_log(node_manager, extra_args=[flag])

    assert os.path.getsize(log_file) >= SEEDED_LOG_SIZE
    assert read_lines(log_file)[0] == seeded[0], "the log should not be trimmed"
