#!/usr/bin/env python3
"""List LTP tests in ltp_all.md that are not covered under ltp_dependence/.

Scans runnable .rs files below `user/src/bin/ltp_dependence/` (that is,
`tasks/*.rs` and `submit_plan.rs`, excluding the explicit `tasks/unrun.rs`
backlog) and collects every quoted literal as a potential test name. The
difference against `ltp_all.md` is then reported, optionally compressed into
`prefix[first-last]` ranges.

Consecutive tests that share the same alphabetic prefix and differ only in a
trailing numeric suffix are compressed into a single range line, e.g.::

    semop01, semop02, semop03, semop04, semop05  →  semop[01-05]
    epoll_wait01, epoll_wait02                   →  epoll_wait[01-02]

Tests whose name has no trailing digits are printed as-is.
"""
from __future__ import annotations

import argparse
import re
from itertools import groupby
from pathlib import Path


# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------

_SUFFIX_RE = re.compile(r'^(.*?)(\d+)$')


def _split(name: str) -> tuple[str, str] | None:
    """Return (prefix, digits) if name ends with digits, else None."""
    m = _SUFFIX_RE.match(name)
    return (m.group(1), m.group(2)) if m else None


def _compress(names: list[str]) -> list[str]:
    """
    Given a sorted list of test names, return a human-readable list where
    runs of consecutively-numbered entries sharing the same prefix are
    collapsed to "prefix[first-last]".

    Non-numeric names or isolated numeric names are emitted unchanged.
    """
    # Group by prefix (or None for names without a numeric suffix).
    def key(name: str) -> str:
        parts = _split(name)
        return parts[0] if parts else name

    result: list[str] = []
    for prefix, group_iter in groupby(names, key=key):
        group = list(group_iter)
        # If only one item or prefix has no numeric suffix, emit as-is.
        if len(group) == 1 or _split(group[0]) is None:
            result.extend(group)
            continue

        # Build runs of consecutive integers.
        # Each item in group is guaranteed to start with `prefix`.
        runs: list[list[str]] = []
        for name in group:
            parts = _split(name)
            if parts is None:
                runs.append([name])
                continue
            num = int(parts[1])
            if runs and _split(runs[-1][-1]) is not None and int(_split(runs[-1][-1])[1]) + 1 == num:
                runs[-1].append(name)
            else:
                runs.append([name])

        for run in runs:
            if len(run) == 1:
                result.append(run[0])
            else:
                first_digits = _split(run[0])[1]   # type: ignore[index]
                last_digits = _split(run[-1])[1]    # type: ignore[index]
                # Keep zero-padding width from original strings.
                result.append(f"{prefix}[{first_digits}-{last_digits}]")

    return result


# ---------------------------------------------------------------------------
# I/O helpers
# ---------------------------------------------------------------------------

def _read_ltp_all(ltp_all_path: Path) -> list[str]:
    lines: list[str] = []
    for raw in ltp_all_path.read_text(encoding="utf-8").splitlines():
        item = raw.strip()
        if not item or item.startswith("#"):
            continue
        lines.append(item)
    return lines


def _read_covered_tests(sources: list[Path]) -> set[str]:
    covered: set[str] = set()
    for path in sources:
        if not path.exists():
            continue
        text = path.read_text(encoding="utf-8")
        for literal in re.findall(r'"([^"]+)"', text):
            head = literal.strip().split()[0] if literal.strip() else ""
            if head:
                covered.add(head)
    return covered


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------

def main() -> int:
    parser = argparse.ArgumentParser(
        description="List LTP tests in ltp_all.md that are not covered in runnable ltp_dependence lists",
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="Repository root path (default: inferred from script location)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Optional output file to write missing tests (one per line, uncompressed)",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
        help="Optional limit for number of compressed entries to print to stdout",
    )
    parser.add_argument(
        "--no-compress",
        action="store_true",
        help="Print every test name individually without range compression",
    )
    args = parser.parse_args()

    root: Path = args.root
    ltp_all_path = root / "ltp_all.md"
    ltp_dir = root / "user" / "src" / "bin" / "ltp_dependence"

    if not ltp_all_path.exists():
        raise SystemExit(f"ltp_all.md not found: {ltp_all_path}")
    if not ltp_dir.exists():
        raise SystemExit(f"ltp_dependence/ not found: {ltp_dir}")

    # Scan runnable .rs files under ltp_dependence/. The unrun module is an
    # explicit backlog, so counting it as covered hides missing tests.
    sources = sorted(
        path for path in ltp_dir.rglob("*.rs") if path.name != "unrun.rs"
    )

    ltp_all = _read_ltp_all(ltp_all_path)
    covered = _read_covered_tests(sources)

    missing = sorted({item for item in ltp_all if item not in covered})

    # --output always writes the raw, uncompressed list.
    if args.output:
        args.output.write_text(
            "\n".join(missing) + ("\n" if missing else ""), encoding="utf-8"
        )

    display = missing if args.no_compress else _compress(missing)

    print(f"Total in ltp_all.md  : {len(ltp_all)}")
    print(f"Covered in runnable lists : {len(covered)}")
    print(f"Missing (raw)        : {len(missing)}")
    print(f"Missing (compressed) : {len(display)}")

    if display:
        to_show = display if args.limit is None else display[: max(
            0, args.limit)]
        print()
        for item in to_show:
            print(item)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
