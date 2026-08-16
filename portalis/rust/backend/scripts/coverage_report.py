#!/usr/bin/env python3
"""Report and gate cargo-llvm-cov JSON output.

Reads the export produced by `cargo llvm-cov --json` and applies the same
thresholds the gate has always used, with one difference that is the whole
point of this file: when it fails, it says what is short and where.

`--fail-under-*` inside cargo-llvm-cov cannot do that. Those flags stop the
run before any summary is printed, and with `--lcov`/`--json` output there is
no summary table at all, so a failing gate emitted exit code 1 and nothing
else. A gate that will not name what it rejects cannot be acted on.

Thresholds are unchanged: functions 100, regions 99, and no uncovered line.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict

# One region in crates/client/src/protocol.rs is reported uncovered by the
# summary no matter what exercises it: ClientProtocol's generic methods are
# compiled once per concrete DeviceSigner, and the region tally counts an
# instantiation the merged profile shows as covered. The count is stable, so
# the region floor stays at 99 rather than pretending it is 100.
DEFAULT_MIN_FUNCTIONS = 100.0
DEFAULT_MIN_REGIONS = 99.0


def demangle(names: list[str]) -> dict[str, str]:
    """Best-effort rustfilt/c++filt demangling; identity when unavailable."""
    for tool in (["rustfilt"], ["c++filt", "-p"]):
        try:
            out = subprocess.run(
                tool,
                input="\n".join(names),
                capture_output=True,
                text=True,
                timeout=30,
                check=True,
            ).stdout.splitlines()
            if len(out) == len(names):
                return dict(zip(names, out))
        except (OSError, subprocess.SubprocessError):
            continue
    return {n: n for n in names}


def strip_disambiguator(name: str) -> str:
    """Drop the crate disambiguator so both builds of a function share a key.

    Rust's v0 mangling embeds a per-compilation hash (`Cs<hash>_`), and the
    legacy scheme embeds `17h<hash>`. The same function compiled with and
    without cfg(test) therefore arrives under two names.
    """
    without_v0 = re.sub(r"Cs[0-9A-Za-z]+_", "", name)
    return re.sub(r"17h[0-9a-f]{16}E?$", "", without_v0)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("json_path")
    parser.add_argument("--min-functions", type=float, default=DEFAULT_MIN_FUNCTIONS)
    parser.add_argument("--min-regions", type=float, default=DEFAULT_MIN_REGIONS)
    args = parser.parse_args()

    with open(args.json_path, encoding="utf-8") as handle:
        report = json.load(handle)

    data = report["data"][0]
    totals = data["totals"]

    functions_pct = totals["functions"]["percent"]
    regions_pct = totals["regions"]["percent"]
    lines_pct = totals["lines"]["percent"]

    print()
    print("Coverage summary")
    for label, key in (("functions", "functions"), ("regions", "regions"), ("lines", "lines")):
        block = totals[key]
        print(
            f"  {label:<10} {block['covered']:>6} / {block['count']:<6} "
            f"({block['percent']:.2f}%)"
        )

    failures: list[str] = []

    # Which files are actually short on functions, according to the same
    # per-file summaries the totals above are built from. Ask this first,
    # because `data.functions` cannot answer it: each crate is compiled twice,
    # with and without cfg(test), so one function appears under two mangled
    # names (the crate disambiguator differs) and the copy belonging to the
    # build that did not run it has a zero count while the function itself is
    # covered. Reading that list raw reports uncovered functions in a report
    # whose own total is 100%, which is worse than not reporting at all.
    short_files = {
        f["filename"]
        for f in data.get("files", [])
        if f["summary"]["functions"]["covered"] < f["summary"]["functions"]["count"]
    }

    if short_files:
        # Names, for the files the summary already condemned. A function is
        # uncovered only if no build of it ran, so sum across both copies.
        executions: dict[tuple[str, str], int] = defaultdict(int)
        for function in data.get("functions", []):
            filenames = [f for f in (function.get("filenames") or []) if f in short_files]
            if not filenames:
                continue
            executions[(filenames[0], strip_disambiguator(function["name"]))] += function.get(
                "count", 0
            )

        uncovered = [key for key, count in executions.items() if count == 0]
        pretty = demangle([name for _file, name in uncovered])
        by_file: dict[str, list[str]] = defaultdict(list)
        for filename, name in uncovered:
            by_file[filename].append(pretty.get(name, name))
        print()
        print(f"Uncovered functions ({len(uncovered)}):", file=sys.stderr)
        for filename in sorted(by_file):
            print(f"  {filename}", file=sys.stderr)
            for name in sorted(by_file[filename]):
                print(f"    {name}", file=sys.stderr)

    # Uncovered lines, located. Read from per-file segments: a segment marks a
    # line, and a count of zero on a region-entry segment means nothing reached
    # it. This is the merged truth across instantiations, which is why the gate
    # reads it rather than the summary's per-instantiation line tally.
    uncovered_lines: dict[str, set[int]] = defaultdict(set)
    for file_report in data.get("files", []):
        filename = file_report["filename"]
        covered_lines: set[int] = set()
        zero_lines: set[int] = set()
        for segment in file_report.get("segments", []):
            line, _col, count, has_count, is_region_entry = segment[:5]
            if not has_count or not is_region_entry:
                continue
            (covered_lines if count > 0 else zero_lines).add(line)
        remaining = zero_lines - covered_lines
        if remaining:
            uncovered_lines[filename] = remaining

    if uncovered_lines:
        total = sum(len(v) for v in uncovered_lines.values())
        print()
        print(f"Uncovered lines ({total}):", file=sys.stderr)
        for filename in sorted(uncovered_lines):
            for line in sorted(uncovered_lines[filename]):
                print(f"  {filename}:{line}", file=sys.stderr)

    if functions_pct < args.min_functions:
        failures.append(f"functions {functions_pct:.2f}% < {args.min_functions:.2f}%")
    if regions_pct < args.min_regions:
        failures.append(f"regions {regions_pct:.2f}% < {args.min_regions:.2f}%")
    if uncovered_lines:
        failures.append(f"{sum(len(v) for v in uncovered_lines.values())} uncovered lines")

    if failures:
        print()
        print("Coverage gate FAILED: " + "; ".join(failures), file=sys.stderr)
        return 1

    print()
    print(
        f"Coverage gate passed "
        f"(functions {functions_pct:.2f}%, regions {regions_pct:.2f}%, lines {lines_pct:.2f}%)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
