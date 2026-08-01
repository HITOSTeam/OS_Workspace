#!/usr/bin/env python3
"""Compare two run_concurrency_focus.sh benchmark logs."""

from __future__ import annotations

import argparse
import math
import re
import statistics
from collections import defaultdict
from pathlib import Path


METRIC_RE = re.compile(
    r"CONCURRENCY_METRIC workload=(\S+) sample=(\d+) "
    r"start_s=([0-9.]+) end_s=([0-9.]+) rc=(\d+)"
)


def read_metrics(path: Path) -> dict[str, list[float]]:
    metrics: dict[str, list[float]] = defaultdict(list)
    for line in path.read_text(errors="replace").splitlines():
        match = METRIC_RE.search(line)
        if not match:
            continue
        workload, _sample, start, end, status = match.groups()
        if status != "0":
            raise ValueError(f"{path}: {workload} returned {status}")
        metrics[workload].append(float(end) - float(start))
    return dict(metrics)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Apply the concurrency integration performance gate"
    )
    parser.add_argument("baseline", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--required-samples", type=int, default=5)
    parser.add_argument("--minimum-improvement", type=float, default=5.0)
    parser.add_argument("--maximum-regression", type=float, default=5.0)
    args = parser.parse_args()

    baseline = read_metrics(args.baseline)
    candidate = read_metrics(args.candidate)
    workloads = sorted(set(baseline) | set(candidate))
    if not workloads:
        raise SystemExit("error: no CONCURRENCY_METRIC records found")

    ratios: list[float] = []
    sample_error = False
    regression_error = False
    print("workload                 baseline_s  candidate_s  change")
    for workload in workloads:
        baseline_values = baseline.get(workload, [])
        candidate_values = candidate.get(workload, [])
        if (
            len(baseline_values) < args.required_samples
            or len(candidate_values) < args.required_samples
        ):
            print(
                f"{workload:24} insufficient samples "
                f"({len(baseline_values)}/{len(candidate_values)})"
            )
            sample_error = True
            continue

        baseline_median = statistics.median(baseline_values)
        candidate_median = statistics.median(candidate_values)
        ratio = candidate_median / baseline_median
        change = (1.0 - ratio) * 100.0
        ratios.append(ratio)
        if change < -args.maximum_regression:
            regression_error = True
        print(
            f"{workload:24} {baseline_median:10.3f}  "
            f"{candidate_median:11.3f}  {change:+6.2f}%"
        )

    if sample_error or not ratios:
        print("GATE: FAIL (insufficient samples)")
        return 2

    geometric_ratio = math.exp(sum(math.log(ratio) for ratio in ratios) / len(ratios))
    aggregate_improvement = (1.0 - geometric_ratio) * 100.0
    print(f"geometric-mean improvement: {aggregate_improvement:+.2f}%")
    if regression_error:
        print("GATE: FAIL (a workload regressed beyond the allowed limit)")
        return 1
    if aggregate_improvement < args.minimum_improvement:
        print("GATE: FAIL (aggregate improvement is below the required minimum)")
        return 1
    print("GATE: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
