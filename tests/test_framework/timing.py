# SPDX-License-Identifier: MIT OR Apache-2.0

"""
tests/test_framework/timing.py

Lightweight timing instrumentation for the functional tests.

Every measured span is appended as one JSON line to
`<timings dir>/<run id>/<worker>.jsonl`, where the timings dir defaults to
`$FLORESTA_TEMP_DIR/timings` and can be overridden with `FLORESTA_TIMINGS_DIR`.
Set `FLORESTA_TIMINGS=0` to disable it.

High frequency events (sleeps, polling loops, RPC calls) are aggregated in
memory and flushed once per test phase, so the instrumentation itself does not
become a bottleneck. Use `timing_report.py` to summarize one or more runs.
"""

import contextlib
import functools
import json
import os
import sys
import threading
import time
from typing import Any, Dict, Optional, Tuple

FRAMEWORK_DIR = os.path.dirname(os.path.abspath(__file__))
TESTS_DIR = os.path.dirname(FRAMEWORK_DIR)

# `time.sleep` before `install_sleep_probe` patches it
_real_sleep = time.sleep

_lock = threading.Lock()
_state: Dict[str, Any] = {
    "file": None,
    "worker": "main",
    "test": None,
    "phase": None,
}
_aggregates: Dict[Tuple, Dict[str, float]] = {}

# Stack of the spans currently open in each thread, so nested spans know their
# parent and inherit the node variant (e.g. an RPC socket wait inside `node.stop`).
_spans = threading.local()


def enabled() -> bool:
    """Whether the instrumentation is recording."""
    return _state["file"] is not None


def default_timings_dir() -> str:
    """Directory where every run gets its own sub-directory."""
    base = os.getenv("FLORESTA_TIMINGS_DIR")
    if base:
        return base

    temp_dir = os.getenv("FLORESTA_TEMP_DIR", "/tmp/floresta-func-tests")
    return os.path.join(temp_dir, "timings")


def is_disabled_by_env() -> bool:
    """`FLORESTA_TIMINGS=0` turns the instrumentation off."""
    return os.getenv("FLORESTA_TIMINGS", "1").lower() in ("0", "false", "no", "off")


def configure(run_dir: str, worker: str):
    """Start recording events for this process into `run_dir`."""
    os.makedirs(run_dir, exist_ok=True)
    path = os.path.join(run_dir, f"{worker}.jsonl")
    # pylint: disable=consider-using-with
    _state["file"] = open(path, "a", encoding="utf-8", buffering=1)
    _state["worker"] = worker


def close():
    """Flush pending aggregates and stop recording."""
    flush_aggregates()
    if _state["file"] is not None:
        _state["file"].close()
        _state["file"] = None


def set_context(test: Optional[str], phase: Optional[str]):
    """Set the test and phase that new events are attributed to."""
    _state["test"] = test
    _state["phase"] = phase


def _write(event: Dict[str, Any]):
    with _lock:
        if _state["file"] is not None:
            _state["file"].write(json.dumps(event, default=str) + "\n")


def record(kind: str, duration: float, **fields):
    """Record a single event that took `duration` seconds."""
    if not enabled():
        return

    event = {
        "kind": kind,
        "duration": round(duration, 6),
        "ts": round(time.time(), 6),
        "worker": _state["worker"],
        "test": _state["test"],
        "phase": _state["phase"],
    }
    stack = getattr(_spans, "stack", [])
    if stack:
        event["parent"] = stack[-1][0]
        for _, parent_fields in reversed(stack):
            if "variant" in parent_fields:
                event["variant"] = parent_fields["variant"]
                break
    event.update(fields)
    _write(event)


def accumulate(kind: str, duration: float, **key):
    """
    Aggregate a high frequency event, keyed by `kind` and `key`, into the
    current test phase. Aggregates are written by `flush_aggregates`.
    """
    if not enabled():
        return

    agg_key = (kind, _state["test"], _state["phase"], tuple(sorted(key.items())))
    with _lock:
        agg = _aggregates.setdefault(agg_key, {"count": 0, "duration": 0.0, "max": 0.0})
        agg["count"] += 1
        agg["duration"] += duration
        agg["max"] = max(agg["max"], duration)


def flush_aggregates():
    """Write every pending aggregate as an event."""
    with _lock:
        pending = list(_aggregates.items())
        _aggregates.clear()

    for (kind, test, phase, key), agg in pending:
        _write(
            {
                "kind": kind,
                "aggregate": True,
                "count": agg["count"],
                "duration": round(agg["duration"], 6),
                "max": round(agg["max"], 6),
                "ts": round(time.time(), 6),
                "worker": _state["worker"],
                "test": test,
                "phase": phase,
                **dict(key),
            }
        )


@contextlib.contextmanager
def span(kind: str, **fields):
    """
    Measure the enclosed block and record it as `kind`. The yielded dict can
    be used to attach extra fields discovered while running the block.
    """
    extra: Dict[str, Any] = {}
    if not hasattr(_spans, "stack"):
        _spans.stack = []
    _spans.stack.append((kind, fields))
    start = time.perf_counter()
    try:
        yield extra
    except BaseException as e:
        extra["error"] = type(e).__name__
        raise
    finally:
        _spans.stack.pop()
        record(kind, time.perf_counter() - start, **fields, **extra)


def timed(kind: str):
    """Decorator that records every call of the function as `kind`, with its call site."""

    def decorator(func):
        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            with span(kind, site=call_site()):
                return func(*args, **kwargs)

        return wrapper

    return decorator


def call_site(skip_framework: bool = True, depth: int = 1) -> str:
    """
    Return `file:line` of the first caller outside this module (and, if
    `skip_framework` is set, outside `test_framework`), relative to `tests/`.
    `depth` is how many frames above the caller of this function to start from.
    """
    # pylint: disable=protected-access
    frame = sys._getframe(depth)
    fallback = None
    while frame is not None:
        filename = os.path.abspath(frame.f_code.co_filename)
        if filename != os.path.abspath(__file__) and filename.startswith(TESTS_DIR):
            site = f"{os.path.relpath(filename, TESTS_DIR)}:{frame.f_lineno}"
            if fallback is None:
                fallback = site
            if not (skip_framework and filename.startswith(FRAMEWORK_DIR)):
                return site
        frame = frame.f_back

    return fallback or "<external>"


def _probed_sleep(seconds):
    start = time.perf_counter()
    try:
        _real_sleep(seconds)
    finally:
        if _state["test"] is not None:
            accumulate(
                "sleep",
                time.perf_counter() - start,
                site=call_site(skip_framework=False),
                origin=call_site(),
            )


def install_sleep_probe():
    """
    Replace `time.sleep` so every sleep made while a test is running is
    attributed to its call site. Must run before test modules are imported,
    so `from time import sleep` also picks up the probe.
    """
    time.sleep = _probed_sleep


def uninstall_sleep_probe():
    """Restore the original `time.sleep`."""
    time.sleep = _real_sleep
