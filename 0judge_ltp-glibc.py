#!/usr/bin/env python3
"""Select the highest-scoring LTP case set that fits a time budget."""

import argparse
import math
import re
import sys


ANSI_ESCAPE = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
START_TIME = re.compile(r"^LTP CASE START (.+) TIME_MS (\d+)$")
END_TIME = re.compile(
    r"^LTP CASE END (.+) TIME_MS (\d+) DURATION_MS (\d+)$"
)
SUMMARY_FIELDS = ("passed", "failed", "broken", "skipped", "warnings")


def empty_case():
    return {
        "passed": 0,
        "success": 0,
        "failed": 0,
        "broken": 0,
        "skipped": 0,
        "warnings": 0,
        "all": 0,
        "return_code": None,
        "summary_seen": False,
        "start_ms": None,
        "end_ms": None,
        "duration_ms": None,
    }


def parse_ltp_log(content: str, group: str):
    """Parse per-case result counts and monotonic-clock durations."""
    result = {}
    testcase = None
    current = empty_case()
    in_summary = False
    inside_group = False
    start_marker = f"#### OS COMP TEST GROUP START {group} ####"
    end_marker = f"#### OS COMP TEST GROUP END {group} ####"

    def save_case():
        nonlocal testcase, current, in_summary
        if not testcase:
            return
        current["success"] = current["passed"]
        if testcase not in result:
            result[testcase] = current
        else:
            saved = result[testcase]
            for field in SUMMARY_FIELDS:
                saved[field] += current[field]
            saved["all"] += current["all"]
            saved["success"] = saved["passed"]
            saved["summary_seen"] = saved["summary_seen"] or current["summary_seen"]
            if current["return_code"] is not None:
                saved["return_code"] = current["return_code"]
            if current["duration_ms"] is not None:
                old_duration = saved["duration_ms"] or 0
                saved["duration_ms"] = old_duration + current["duration_ms"]
                saved["end_ms"] = current["end_ms"]
        testcase = None
        current = empty_case()
        in_summary = False

    for raw_line in content.splitlines():
        line = ANSI_ESCAPE.sub("", raw_line).strip()

        if line == start_marker:
            print(f"找到{start_marker}")
            inside_group = True
            continue
        if not inside_group:
            continue
        if line == end_marker:
            save_case()
            break

        if line.startswith("RUN LTP CASE "):
            save_case()
            testcase = line[len("RUN LTP CASE "):].strip()
            continue

        if testcase and line.startswith(f"FAIL LTP CASE {testcase}"):
            parts = line.split()
            try:
                current["return_code"] = int(parts[-1])
            except (ValueError, IndexError):
                current["return_code"] = None
            save_case()
            continue

        if not testcase:
            continue

        match = START_TIME.match(line)
        if match and match.group(1) == testcase:
            current["start_ms"] = int(match.group(2))
            continue

        match = END_TIME.match(line)
        if match and match.group(1) == testcase:
            current["end_ms"] = int(match.group(2))
            current["duration_ms"] = int(match.group(3))
            continue

        if line == "Summary:":
            in_summary = True
            current["summary_seen"] = True
            continue

        if in_summary:
            if not line:
                in_summary = False
                continue
            parts = line.split()
            if len(parts) >= 2 and parts[0] in SUMMARY_FIELDS:
                try:
                    value = int(parts[1])
                except ValueError:
                    in_summary = False
                    continue
                current[parts[0]] += value
                current["all"] += value
            else:
                in_summary = False

    save_case()
    return result


def select_best_cases(result, budget_seconds: int, include_zero: bool):
    """0/1 knapsack: maximize TPASS, then choose the shortest total time."""
    candidates = []
    # 首先对结果进行过滤
    count_temp = 0
    for name, stats in result.items():
        duration_ms = stats["duration_ms"]
        if duration_ms is None:
            continue
        if duration_ms > 100000 and stats["passed"] < 2:
            # 超过1分半,TPASS还少的不要
            continue
        if stats["passed"] == 0 :
            continue
        count_temp += stats["passed"]
        duration_seconds = max(1, math.ceil(duration_ms / 1000))
        if duration_seconds <= budget_seconds:
            candidates.append((name, stats, duration_seconds))
    print(f"本次的总分： {count_temp}")
    # 创建DP数组，求解0 1 背包问题
    unreachable = -1
    #加1 是要考虑从0开始的
    scores = [unreachable] * (budget_seconds + 1)
    # 初始化0
    scores[0] = 0
    choices = []

    # 开始对任务进行遍历，求解问题
    for _, stats, cost in candidates:
        took = bytearray(budget_seconds + 1)
        value = stats["passed"]
        # 反着遍历，正向遍历会算两遍
        for elapsed in range(budget_seconds, cost - 1, -1):
            previous = scores[elapsed - cost]
            if previous == unreachable:
                continue
            candidate_score = previous + value
            if candidate_score > scores[elapsed]:
                scores[elapsed] = candidate_score
                took[elapsed] = 1
        choices.append(took)

    best_score = max(scores)
    best_elapsed = min(i for i, score in enumerate(scores) if score == best_score)
    selected = []
    elapsed = best_elapsed
    for index in range(len(candidates) - 1, -1, -1):
        if choices[index][elapsed]:
            selected.append(candidates[index])
            elapsed -= candidates[index][2]

    # 按照时间最大的来排序
    selected.sort(key=lambda item: (-item[1]["duration_ms"], item[2], item[0]))
    return selected, best_score, best_elapsed


def rust_string(value: str) -> str:
    return '"' + value.replace("\\", "\\\\").replace('"', '\\"') + '"'


def emit_rust_array(selected, arch: str, score: int, elapsed: int, budget: int):
    const_name = "RISCV_LTP_CASES" if arch == "riscv" else "LOONGARCH_LTP_CASES"
    print(f"pub const {const_name}: &[&str] = &[")
    for name, stats, seconds in selected:
        print(
            f"    {rust_string(name)}, "
            # "// TPASS={stats['success']} "
            # f"duration={seconds}s"
        )
    print("];")

    print(
        f"generated {const_name}: {len(selected)} cases, passed={score}, "
        f"estimated={elapsed}s, budget={budget}s",
        file=sys.stderr,
    )


def main():
    parser = argparse.ArgumentParser(
        description=(
            "Read an LTP log from stdin and emit the maximum-passed Rust array "
            "that fits the requested time budget."
        )
    )
    parser.add_argument("--arch", choices=("riscv", "loongarch"), required=True)
    parser.add_argument(
        "--group",
        choices=("ltp-musl", "ltp-glibc"),
        default="ltp-musl",
    )
    parser.add_argument("--budget-minutes", type=float, default=20.0)
    parser.add_argument(
        "--reserve-seconds",
        type=int,
        default=30,
        help="允许的延迟时间，考虑评测机会慢",
    )
    parser.add_argument("--include-zero", action="store_true")
    args = parser.parse_args()

    total_budget = math.floor(args.budget_minutes * 60)
    budget = total_budget - args.reserve_seconds
    if total_budget <= 0 or args.reserve_seconds < 0 or budget <= 0:
        parser.error("time budget must be larger than the safety reserve")

    content = sys.stdin.buffer.read().decode("latin-1")
    result_musl = parse_ltp_log(content, "ltp-musl")
    
    result = result_musl
    result_glibc = parse_ltp_log(content, "ltp-glibc")
    
    result = None
    if args.group =="ltp-glibc":
        result = result_glibc
    else:
        result = result_musl
    
    count_temp = 0
    # count_glibc = 0
    
    for name, stats in result_musl.items():
        
        if stats["passed"] == 0 :
            continue
        count_temp += stats["passed"]
        # score_glibc =  result_glibc[name]["passed"]
    #     count_glibc += score_glibc
    #     if score_glibc != stats["passed"]:
    #         print(f"{name} 有问题,musl: {stats["passed"]},glibc: {score_glibc}")

    # print(f"本次的musl总分： {count_temp} glibc: {count_glibc}")
    
    if not result:
        parser.error(f"no cases found in group {args.group}")

    timed = sum(stats["duration_ms"] is not None for stats in result.values())
    if timed == 0:
        parser.error("no completed cases with LTP CASE START/END timestamps found")

    selected, score, elapsed = select_best_cases(result, budget, args.include_zero)
    emit_rust_array(selected, args.arch, score, elapsed, budget)
    missing = len(result) - timed
    if missing:
        print(f"excluded {missing} cases without an end timestamp", file=sys.stderr)


if __name__ == "__main__":
    main()
