# SPDX-License-Identifier: MIT OR Apache-2.0

"""
tests/test_framework/timing_report.py

Summarize the timing data recorded by `test_framework/timing.py` as Markdown.

Accepts run directories (the ones containing `meta.json`) or directories
containing several runs, and averages everything over the selected runs, so
repeated executions smooth out noise.

Usage:
    uv run python tests/test_framework/timing_report.py [PATH ...] [--last N]
        [--label LABEL] [--top N] [--output FILE]

Without PATH, `$FLORESTA_TIMINGS_DIR` or `$FLORESTA_TEMP_DIR/timings` is used.
"""

# pylint: disable=too-many-locals

import argparse
import json
import math
import os
import statistics
import sys
from collections import defaultdict
from typing import Any, Callable, Dict, Iterable, List, Tuple

RPC_VARIANTS = {
    "FlorestaRPC": "florestad",
    "BitcoinRPC": "bitcoind",
    "UtreexoRPC": "utreexod",
}

START_STAGES = [
    ("daemon.spawn", "spawn process"),
    ("daemon.start_fixed_sleep", "fixed sleep after spawn"),
    ("rpc.wait_socket_open", "wait RPC socket open"),
    ("node.first_rpc", "first RPC call"),
    ("node.electrum_ping", "electrum ping"),
]

STOP_STAGES = [
    ("rpc.stop_call", "`stop` RPC call"),
    ("rpc.stop_wait_shutdown", "wait RPC socket closed (daemon shutdown)"),
    ("node.process_exit", "wait process exit"),
    ("rpc.wait_socket_closed", "re-check socket closed"),
]


# pylint: disable=too-few-public-methods
class Run:
    """A single pytest invocation recorded in its own directory."""

    def __init__(self, path: str):
        self.path = path
        self.run_id = os.path.basename(path.rstrip("/"))
        self.meta = _load_json(os.path.join(path, "meta.json"))
        self.session = _load_json(os.path.join(path, "session.json"))
        self.events: List[Dict[str, Any]] = []

        for name in sorted(os.listdir(path)):
            if not name.endswith(".jsonl"):
                continue
            with open(os.path.join(path, name), encoding="utf-8") as f:
                for line in f:
                    line = line.strip()
                    if line:
                        self.events.append(json.loads(line))


def _load_json(path: str) -> Dict[str, Any]:
    if not os.path.exists(path):
        return {}
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def find_runs(paths: Iterable[str]) -> List[Run]:
    """Collect every run directory under the given paths, oldest first."""
    run_dirs = set()
    for path in paths:
        if os.path.exists(os.path.join(path, "meta.json")):
            run_dirs.add(os.path.abspath(path))
            continue
        if not os.path.isdir(path):
            continue
        for name in os.listdir(path):
            candidate = os.path.join(path, name)
            if os.path.exists(os.path.join(candidate, "meta.json")):
                run_dirs.add(os.path.abspath(candidate))

    return [Run(d) for d in sorted(run_dirs, key=os.path.basename)]


def percentile(values: List[float], pct: float) -> float:
    """Nearest-rank percentile."""
    if not values:
        return math.nan
    ordered = sorted(values)
    rank = max(0, math.ceil(pct / 100 * len(ordered)) - 1)
    return ordered[rank]


def fmt(seconds: float) -> str:
    """Format a duration in seconds for a table cell."""
    if seconds is None or (isinstance(seconds, float) and math.isnan(seconds)):
        return "-"
    if abs(seconds) < 1:
        return f"{seconds * 1000:.1f}ms"
    return f"{seconds:.2f}s"


def stats_row(values: List[float], runs: int) -> List[str]:
    """n, n/run, mean, stdev, p50, p95, max, total/run."""
    if not values:
        return ["0", "0", "-", "-", "-", "-", "-", "-"]
    stdev = statistics.stdev(values) if len(values) > 1 else 0.0
    return [
        str(len(values)),
        f"{len(values) / runs:.1f}",
        fmt(statistics.mean(values)),
        fmt(stdev),
        fmt(percentile(values, 50)),
        fmt(percentile(values, 95)),
        fmt(max(values)),
        fmt(sum(values) / runs),
    ]


STATS_HEADER = ["n", "n/run", "mean", "stdev", "p50", "p95", "max", "total/run"]


def table(header: List[str], rows: List[List[str]]) -> str:
    """Render a Markdown table."""
    lines = [
        "| " + " | ".join(header) + " |",
        "|" + "|".join("---" for _ in header) + "|",
    ]
    lines += ["| " + " | ".join(str(c) for c in row) + " |" for row in rows]
    return "\n".join(lines)


def events_of(runs: List[Run], predicate: Callable[[Dict], bool]) -> List[Dict]:
    """Every event, across runs, matching `predicate`."""
    return [e for run in runs for e in run.events if predicate(e)]


def variant_of(event: Dict[str, Any]) -> str:
    """Node variant of an event, falling back to the RPC client class."""
    return event.get("variant") or RPC_VARIANTS.get(event.get("rpc"), "?")


def section_runs(runs: List[Run]) -> str:
    """One row per run, to spot outliers and compare hosts."""
    rows = []
    for run in runs:
        meta, session = run.meta, run.session
        ci = meta.get("ci", {})
        host = ci.get("RUNNER_NAME") or meta.get("platform", "?")
        rows.append(
            [
                run.run_id,
                meta.get("label") or "",
                meta.get("git_commit") or "",
                f"{host} ({meta.get('cpu_count')} cpus)",
                str(meta.get("numprocesses")),
                fmt(session.get("wall_time")),
                str(session.get("testscollected", "?")),
                str(session.get("testsfailed", "?")),
            ]
        )

    walls = [r.session["wall_time"] for r in runs if "wall_time" in r.session]
    out = table(
        ["run", "label", "commit", "host", "-n", "wall", "tests", "failed"], rows
    )
    if walls:
        stdev = statistics.stdev(walls) if len(walls) > 1 else 0.0
        out += (
            f"\n\nWall time: mean **{fmt(statistics.mean(walls))}**, "
            f"stdev {fmt(stdev)}, min {fmt(min(walls))}, max {fmt(max(walls))}"
        )

    failed = [r.run_id for r in runs if r.session.get("testsfailed")]
    if failed:
        out += (
            "\n\n> ⚠️ Runs with failures (if `-x` stopped them early, the "
            f"`tests` column is lower and totals cover fewer tests): {', '.join(failed)}"
        )
    return out


def section_overview(runs: List[Run]) -> str:
    """Where the worker time goes, averaged per run."""
    n = len(runs)
    rows = []

    phases = events_of(runs, lambda e: e["kind"] == "test.phase")
    total = sum(e["duration"] for e in phases)
    for phase in ("setup", "call", "teardown"):
        value = sum(e["duration"] for e in phases if e["phase"] == phase)
        rows.append([f"test {phase}", fmt(value / n), _pct(value, total)])
    rows.append(["**all test phases**", f"**{fmt(total / n)}**", "100%"])

    for kind, label in (("node.start", "node start"), ("node.stop", "node stop")):
        by_variant = defaultdict(float)
        for e in events_of(runs, lambda e, k=kind: e["kind"] == k):
            by_variant[e["variant"]] += e["duration"]
        for variant, value in sorted(by_variant.items(), key=lambda kv: -kv[1]):
            rows.append([f"{label}: {variant}", fmt(value / n), _pct(value, total)])

    sleeps = events_of(runs, lambda e: e["kind"] == "sleep")
    framework_sleep = sum(
        e["duration"] for e in sleeps if e["site"].startswith("test_framework/")
    )
    test_sleep = sum(e["duration"] for e in sleeps) - framework_sleep
    rows.append(
        [
            "time.sleep in test_framework",
            fmt(framework_sleep / n),
            _pct(framework_sleep, total),
        ]
    )
    rows.append(
        ["time.sleep in tests/fixtures", fmt(test_sleep / n), _pct(test_sleep, total)]
    )

    rpc = sum(
        e["duration"] for e in events_of(runs, lambda e: e["kind"] == "rpc.request")
    )
    rows.append(["RPC requests (client side)", fmt(rpc / n), _pct(rpc, total)])

    return (
        "Summed over all workers and averaged per run. Rows overlap: node "
        "start/stop and sleeps happen inside test phases.\n\n"
        + table(["bucket", "worker time/run", "% of test time"], rows)
    )


def _pct(value: float, total: float) -> str:
    return f"{100 * value / total:.0f}%" if total else "-"


def section_node_lifecycle(runs: List[Run]) -> str:
    """Start and stop of each daemon, broken down into stages."""
    n = len(runs)
    parts = []
    variants = sorted(
        {e["variant"] for e in events_of(runs, lambda e: e["kind"] == "node.start")}
    )

    for variant in variants:
        rows = []
        for existing in (False, True):
            starts = events_of(
                runs,
                lambda e, x=existing, v=variant: e["kind"] == "node.start"
                and e["variant"] == v
                and e.get("existing_chain_state") == x
                and "error" not in e,
            )
            if not starts:
                continue
            label = (
                "**start** (existing chain state)"
                if existing
                else "**start** (new chain state)"
            )
            rows.append([label] + stats_row([e["duration"] for e in starts], n))

        failed_starts = events_of(
            runs,
            lambda e, v=variant: e["kind"] == "node.start"
            and e["variant"] == v
            and "error" in e,
        )
        if failed_starts:
            rows.append(
                ["**start that raised** (excluded above)"]
                + stats_row([e["duration"] for e in failed_starts], n)
            )

        for kind, label in START_STAGES:
            values = [
                e["duration"]
                for e in events_of(
                    runs,
                    lambda e, k=kind, v=variant: e["kind"] == k
                    and e.get("parent") == "node.start"
                    and variant_of(e) == v,
                )
            ]
            if values:
                rows.append([f"↳ {label}"] + stats_row(values, n))

        stops = events_of(
            runs, lambda e, v=variant: e["kind"] == "node.stop" and e["variant"] == v
        )
        if stops:
            rows.append(["**stop**"] + stats_row([e["duration"] for e in stops], n))

        for kind, label in STOP_STAGES:
            values = [
                e["duration"]
                for e in events_of(
                    runs,
                    lambda e, k=kind, v=variant: e["kind"] == k
                    and e.get("parent") == "node.stop"
                    and variant_of(e) == v,
                )
            ]
            if values:
                rows.append([f"↳ {label}"] + stats_row(values, n))

        cpu = [
            e.get("lifetime_cpu_user", 0) + e.get("lifetime_cpu_sys", 0)
            for e in stops
            if "lifetime_cpu_user" in e
        ]
        if cpu:
            rows.append(["daemon lifetime CPU (user+sys)"] + stats_row(cpu, n))

        parts.append(f"### {variant}\n\n" + table(["stage"] + STATS_HEADER, rows))

    retries = events_of(
        runs,
        lambda e: e["kind"] == "framework.run_node_attempt" and e.get("attempt", 0) > 0,
    )
    terminated = events_of(
        runs, lambda e: e["kind"] == "node.stop" and e.get("method") == "terminate"
    )
    timeouts = events_of(runs, lambda e: e.get("timed_out"))
    failed_attempts = events_of(
        runs, lambda e: e["kind"] == "framework.run_node_attempt" and "error" in e
    )
    failed_by_test = defaultdict(int)
    for e in failed_attempts:
        failed_by_test[e["test"]] += 1
    notes = [
        f"- start attempts that raised: **{len(failed_attempts)}**, "
        f"{fmt(sum(e['duration'] for e in failed_attempts) / n)}/run "
        "(tests expecting a failed start still pay the fixed sleep on each of "
        "`run_node`'s 3 attempts)"
        + "".join(
            f"\n  - `{test}`: {count / n:.0f}/run"
            for test, count in sorted(failed_by_test.items())
        ),
        f"- start retries (`run_node` attempt > 0): **{len(retries)}**",
        f"- stops that fell back to `terminate()`: **{len(terminated)}**",
        f"- socket waits that timed out: **{len(timeouts)}**",
    ]
    return "\n\n".join(parts) + "\n\n" + "\n".join(notes)


def section_tests(runs: List[Run], top: int) -> str:
    """Slowest tests, with their phases and node starts, averaged over runs."""
    per_test: Dict[str, Dict[str, List[float]]] = defaultdict(lambda: defaultdict(list))
    for run in runs:
        totals: Dict[str, Dict[str, float]] = defaultdict(lambda: defaultdict(float))
        for e in run.events:
            if e["kind"] == "test.phase":
                totals[e["test"]][e["phase"]] += e["duration"]
                totals[e["test"]]["total"] += e["duration"]
            elif e["kind"] == "node.start" and e.get("test"):
                totals[e["test"]][f"starts:{e['variant']}"] += 1
        for test, values in totals.items():
            for key, value in values.items():
                per_test[test][key].append(value)

    def mean(values: List[float]) -> float:
        return statistics.mean(values) if values else 0.0

    ranked = sorted(per_test.items(), key=lambda kv: -mean(kv[1]["total"]))
    rows = []
    for test, values in ranked[:top]:
        starts = ", ".join(
            f"{key.split(':', 1)[1]}×{mean(v):.0f}"
            for key, v in sorted(values.items())
            if key.startswith("starts:")
        )
        stdev = statistics.stdev(values["total"]) if len(values["total"]) > 1 else 0.0
        rows.append(
            [
                f"`{test}`",
                str(len(values["total"])),
                f"**{fmt(mean(values['total']))}**",
                fmt(stdev),
                fmt(mean(values["setup"])),
                fmt(mean(values["call"])),
                fmt(mean(values["teardown"])),
                starts,
            ]
        )

    return table(
        ["test", "runs", "total", "stdev", "setup", "call", "teardown", "node starts"],
        rows,
    )


def _aggregated(
    runs: List[Run], kind: str, key: Callable[[Dict], Tuple]
) -> List[Tuple[Tuple, float, int, float]]:
    """Group aggregated events by `key`: (key, total, count, max) summed over runs."""
    groups: Dict[Tuple, List[float]] = defaultdict(lambda: [0.0, 0, 0.0])
    for e in events_of(runs, lambda e: e["kind"] == kind):
        group = groups[key(e)]
        group[0] += e["duration"]
        group[1] += e.get("count", 1)
        group[2] = max(group[2], e.get("max", e["duration"]))
    return sorted(
        ((k, v[0], v[1], v[2]) for k, v in groups.items()), key=lambda item: -item[1]
    )


def section_sleeps(runs: List[Run], top: int) -> str:
    """Every `time.sleep` made during tests, by call site."""
    n = len(runs)
    rows = [
        [
            f"`{site}`",
            f"`{origin}`" if origin != site else "",
            f"{count / n:.1f}",
            fmt(total / count),
            fmt(max_),
            f"**{fmt(total / n)}**",
        ]
        for (site, origin), total, count, max_ in _aggregated(
            runs, "sleep", lambda e: (e["site"], e.get("origin", e["site"]))
        )[:top]
    ]
    return (
        "`site` is who called `time.sleep`; `origin` is the first caller outside "
        "`test_framework` (the test or fixture that triggered it).\n\n"
        + table(["site", "origin", "calls/run", "mean", "max", "total/run"], rows)
    )


def section_waits(runs: List[Run], top: int) -> str:
    """Polling loops (`wait_until`) and framework helpers that wait on nodes."""
    n = len(runs)
    rows = [
        [
            f"`{site}`",
            f"`{origin}`",
            str(interval),
            f"{count / n:.1f}",
            fmt(total / count),
            fmt(max_),
            f"**{fmt(total / n)}**",
        ]
        for (site, origin, interval), total, count, max_ in _aggregated(
            runs,
            "wait_until",
            lambda e: (e["site"], e.get("origin", ""), e.get("interval", "0.05")),
        )[:top]
    ]
    out = "#### wait_until\n\n" + table(
        ["site", "origin", "interval", "calls/run", "mean", "max", "total/run"], rows
    )

    helper_rows = []
    kinds = sorted(
        {
            e["kind"]
            for run in runs
            for e in run.events
            if e["kind"].startswith("framework.")
        }
    )
    for kind in kinds:
        events = events_of(runs, lambda e, k=kind: e["kind"] == k)
        helper_rows.append(
            [f"`{kind}`"] + stats_row([e["duration"] for e in events], n)
        )

    out += "\n\n#### framework helpers\n\n" + table(
        ["helper"] + STATS_HEADER, helper_rows
    )

    peers = events_of(
        runs, lambda e: e["kind"] == "framework.wait_for_peers_connections"
    )
    attempts = [e["attempts"] for e in peers if "attempts" in e]
    if attempts:
        slow = sum(1 for a in attempts if a > 10)
        out += (
            f"\n\n`wait_for_peers_connections`: mean {statistics.mean(attempts):.1f} "
            f"attempts, max {max(attempts)}, {slow} calls past 10 attempts "
            "(each extra attempt adds a 1s sleep)."
        )
    return out


def section_rpc(runs: List[Run], top: int) -> str:
    """Client-side RPC latency, by daemon and method."""
    n = len(runs)
    rows = [
        [
            RPC_VARIANTS.get(rpc, rpc),
            f"`{method}`",
            f"{count / n:.1f}",
            fmt(total / count),
            fmt(max_),
            f"**{fmt(total / n)}**",
        ]
        for (rpc, method), total, count, max_ in _aggregated(
            runs, "rpc.request", lambda e: (e["rpc"], e["method"])
        )[:top]
    ]
    return table(["daemon", "method", "calls/run", "mean", "max", "total/run"], rows)


def section_noise(runs: List[Run]) -> str:
    """Host load while tests ran, to judge whether a run was noisy."""
    rows = []
    for run in runs:
        loads = [
            e["loadavg"]
            for e in run.events
            if e["kind"] == "test.phase" and e.get("loadavg") is not None
        ]
        start = run.meta.get("loadavg_start") or [None]
        end = run.session.get("loadavg_end") or [None]
        rows.append(
            [
                run.run_id,
                str(run.meta.get("cpu_count")),
                f"{start[0]:.2f}" if start[0] is not None else "-",
                f"{statistics.mean(loads):.2f}" if loads else "-",
                f"{max(loads):.2f}" if loads else "-",
                f"{end[0]:.2f}" if end[0] is not None else "-",
            ]
        )
    return table(
        ["run", "cpus", "load 1m start", "load 1m mean", "load 1m max", "load 1m end"],
        rows,
    )


def build_report(runs: List[Run], top: int) -> str:
    """Assemble the full Markdown report."""
    sections = [
        ("Runs", section_runs(runs)),
        ("Where the time goes", section_overview(runs)),
        ("Node start/stop", section_node_lifecycle(runs)),
        ("Slowest tests", section_tests(runs, top)),
        ("Sleeps", section_sleeps(runs, top)),
        ("Waits", section_waits(runs, top)),
        ("RPC calls", section_rpc(runs, top)),
        ("Host load (noise)", section_noise(runs)),
    ]
    body = "\n\n".join(f"## {title}\n\n{content}" for title, content in sections)
    return f"# Functional tests timing report ({len(runs)} runs)\n\n{body}\n"


def main() -> int:
    """Entry point."""
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n", maxsplit=1)[0])
    parser.add_argument("paths", nargs="*", help="run directories or their parent")
    parser.add_argument("--last", type=int, help="only use the N most recent runs")
    parser.add_argument(
        "--label", help="only use runs with this FLORESTA_TIMINGS_LABEL"
    )
    parser.add_argument("--top", type=int, default=25, help="rows in ranked tables")
    parser.add_argument("--output", help="write the report here instead of stdout")
    args = parser.parse_args()

    if not args.paths:
        # Imported here so collecting this module under pytest has no side effects
        sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        # pylint: disable=import-outside-toplevel
        from test_framework.timing import default_timings_dir

        args.paths = [default_timings_dir()]

    runs = find_runs(args.paths)
    if args.label:
        runs = [r for r in runs if r.meta.get("label") == args.label]
    if args.last:
        runs = runs[-args.last :]
    if not runs:
        print("No timing runs found", file=sys.stderr)
        return 1

    report = build_report(runs, args.top)
    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(report)
    else:
        print(report)
    return 0


if __name__ == "__main__":
    sys.exit(main())
