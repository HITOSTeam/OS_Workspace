#!/usr/bin/env python3
"""Find abnormal or skipped LTP results from a log.

Usage:
  python3 tools/find_ltp_error.py <file>

If no file is provided, reads from stdin.
"""
from __future__ import annotations

import re
import sys
from typing import Iterable

PATTERN = re.compile(
    r"^\s*FAIL\s+LTP\s+CASE\s+(?P<case>[^:]+?)\s*:\s*(?P<count>\d+)\s*$")
RUN_CASE_PATTERN = re.compile(r"^\s*RUN\s+LTP\s+CASE\s+(?P<case>.+?)\s*$")
SKIPPED_SUMMARY_PATTERN = re.compile(r"^\s*skipped\s+(?P<count>\d+)\s*$")


def iter_lines(path: str | None) -> Iterable[str]:
    if path:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            yield from f
    else:
        yield from sys.stdin


def print_fail_case() -> int:
    path = sys.argv[1] if len(sys.argv) > 1 else None
    found = False
    for line in iter_lines(path):
        m = PATTERN.match(line)
        if not m:
            continue
        count = int(m.group("count"))
        if count != 0:
            found = True
            case = m.group("case").strip()
            print(f"{case}: {count}")
    return 1 if found else 0


# 这个函数会打印失败/跳过/配置不满足的行，并在 summary 中附带 case 名称：
# waitpid01.c:116: TFAIL: WIFSIGNALED() not set in status (exited with 0)
# skipped  1    [case=sysfs04]
def print_not_success_line() -> int:
    path = sys.argv[1] if len(sys.argv) > 1 else None
    found = False
    current_case: str | None = None
    for line in iter_lines(path):
        run_match = RUN_CASE_PATTERN.match(line)
        if run_match:
            current_case = run_match.group("case").strip()
            continue
        if (
            "TFAIL" in line
            or "TBROK" in line
            or "TSKIP" in line
            or "TCONF" in line
        ):
            found = True
            print(line.strip())
            continue
        skipped_match = SKIPPED_SUMMARY_PATTERN.match(line)
        if skipped_match and int(skipped_match.group("count")) != 0:
            found = True
            case_suffix = f"    [case={current_case}]" if current_case else ""
            print(f"{line.strip()}{case_suffix}")
    return 1 if found else 0


if __name__ == "__main__":
    print("Checking for non-zero FAIL LTP CASE counts...")
    print_fail_case()
    print("Checking for TFAIL/TBROK/TSKIP/TCONF/skipped lines...")
    print_not_success_line()
