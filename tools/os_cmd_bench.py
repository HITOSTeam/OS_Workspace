#!/usr/bin/env python3
"""Measure the wall-clock time of a command script inside CongCore QEMU.

Examples:
  python3 tools/os_cmd_bench.py --cmd './hackbench -l 1' --repeat 5
  python3 tools/os_cmd_bench.py --script tools/bench.sh --repeat 3 --raw-log bench.log
"""

from __future__ import annotations

import argparse
import json
import os
import pty
import re
import select
import shlex
import signal
import statistics
import subprocess
import sys
import time
import uuid
from pathlib import Path
from typing import Callable


ANSI_RE = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]")
PROMPT_RE = re.compile(r"CongCore:[^\r\n]*[$#]")


class GuestTimeout(RuntimeError):
    pass


class QemuSession:
    def __init__(
        self,
        root: Path,
        args: argparse.Namespace,
        raw_log,
    ) -> None:
        self.root = root
        self.args = args
        self.raw_log = raw_log
        self.master_fd: int | None = None
        self.proc: subprocess.Popen[bytes] | None = None
        self.text_tail = ""
        self.boot_seconds: float | None = None

    def start(self) -> None:
        master_fd, slave_fd = pty.openpty()
        env = os.environ.copy()
        env["ARCH"] = self.args.arch
        cmd = [
            "make",
            "-C",
            "os",
            "run_ext4",
            f"LOG={self.args.log}",
            f"SMP={self.args.smp}",
            f"MEM={self.args.mem}",
            f"SUBMIT={self.args.submit}",
            f"EXT4_REBUILD={self.args.ext4_rebuild}",
            f"EXT4_SIZE={self.args.ext4_size}",
            "QEMU_TIMEOUT=0",
        ]
        for item in self.args.make_var:
            cmd.append(item)

        started = time.perf_counter()
        self.proc = subprocess.Popen(
            cmd,
            cwd=self.root,
            env=env,
            stdin=slave_fd,
            stdout=slave_fd,
            stderr=slave_fd,
            close_fds=True,
            start_new_session=True,
        )
        os.close(slave_fd)
        self.master_fd = master_fd
        self._read_until(
            lambda text, _now: PROMPT_RE.search(text) is not None,
            self.args.boot_timeout,
            "guest prompt",
        )
        self.boot_seconds = time.perf_counter() - started

    def run_script(self, script: str, label: str, timeout: float) -> dict[str, object]:
        run_id = uuid.uuid4().hex
        marker_prefix = "__OS_CMD_BENCH"
        begin_marker = f"{marker_prefix}_BEGIN_{run_id}__"
        end_marker = f"{marker_prefix}_END_{run_id}__"
        wrapper = self._wrap_script(script, marker_prefix, run_id)

        begin_seen = False
        begin_time = 0.0
        end_match: re.Match[str] | None = None

        def done(text: str, now: float) -> bool:
            nonlocal begin_seen, begin_time, end_match
            if not begin_seen and begin_marker in text:
                begin_seen = True
                begin_time = now
            if begin_seen:
                end_match = re.search(re.escape(end_marker) + r":(-?\d+)", text)
                return end_match is not None
            return False

        self.text_tail = ""
        self.write_line(self.args.guest_shell)
        self.write_block(wrapper + "\n")
        self._read_until(done, timeout, label)
        elapsed = time.perf_counter() - begin_time
        rc = int(end_match.group(1)) if end_match else -1
        self._read_until(
            lambda text, _now: PROMPT_RE.search(text) is not None,
            self.args.prompt_timeout,
            "guest prompt after bench",
        )
        return {
            "label": label,
            "elapsed_seconds": elapsed,
            "return_code": rc,
        }

    def stop(self) -> None:
        if self.proc is None:
            return
        if self.proc.poll() is None and self.master_fd is not None:
            try:
                self.write_line(self.args.poweroff_cmd)
                self.proc.wait(timeout=self.args.poweroff_timeout)
            except Exception:
                self._terminate()
        if self.master_fd is not None:
            try:
                os.close(self.master_fd)
            except OSError:
                pass
            self.master_fd = None

    def _terminate(self) -> None:
        if self.proc is None or self.proc.poll() is not None:
            return
        try:
            os.killpg(self.proc.pid, signal.SIGTERM)
            self.proc.wait(timeout=5)
        except Exception:
            try:
                os.killpg(self.proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            self.proc.wait(timeout=5)

    def write_line(self, line: str) -> None:
        if self.master_fd is None:
            raise RuntimeError("QEMU session is not started")
        os.write(self.master_fd, line.encode("utf-8") + b"\n")

    def write_block(self, text: str) -> None:
        if self.master_fd is None:
            raise RuntimeError("QEMU session is not started")
        os.write(self.master_fd, text.encode("utf-8"))

    def _read_until(
        self,
        predicate: Callable[[str, float], bool],
        timeout: float,
        what: str,
    ) -> None:
        if self.master_fd is None or self.proc is None:
            raise RuntimeError("QEMU session is not started")
        deadline = time.perf_counter() + timeout
        if predicate(self.text_tail, time.perf_counter()):
            return
        while True:
            if self.proc.poll() is not None:
                raise RuntimeError(f"QEMU exited while waiting for {what}")
            remaining = deadline - time.perf_counter()
            if remaining <= 0:
                raise GuestTimeout(
                    f"timed out waiting for {what}; recent output:\n"
                    f"{self.text_tail[-4000:]}"
                )
            readable, _, _ = select.select([self.master_fd], [], [], min(0.2, remaining))
            if not readable:
                continue
            chunk = os.read(self.master_fd, 65536)
            if not chunk:
                continue
            now = time.perf_counter()
            if self.raw_log:
                self.raw_log.write(chunk)
                self.raw_log.flush()
            if self.args.verbose:
                sys.stdout.buffer.write(chunk)
                sys.stdout.buffer.flush()
            clean = ANSI_RE.sub(b"", chunk).decode("utf-8", errors="replace")
            self.text_tail = (self.text_tail + clean)[-65536:]
            if predicate(self.text_tail, now):
                return

    def _wrap_script(self, script: str, marker_prefix: str, run_id: str) -> str:
        lines = [
            f"M={marker_prefix}",
            f"echo ${{M}}_BEGIN_{run_id}__",
            "(",
        ]
        if self.args.set_e:
            lines.append("set -e")
        if self.args.guest_cwd:
            lines.append(f"cd {shlex.quote(self.args.guest_cwd)}")
        lines.append(script.rstrip())
        lines.extend(
            [
                ")",
                "rc=$?",
                f"echo ${{M}}_END_{run_id}__:$rc",
                "exit $rc",
            ]
        )
        return "\n".join(lines)


def load_script(args: argparse.Namespace) -> str:
    parts: list[str] = []
    if args.script:
        parts.append(Path(args.script).read_text(encoding="utf-8"))
    if args.cmd:
        parts.extend(args.cmd)
    if not parts:
        raise SystemExit("provide --script or at least one --cmd")
    return "\n".join(parts).strip() + "\n"


def summarize(samples: list[dict[str, object]]) -> dict[str, float | int | None]:
    values = [float(s["elapsed_seconds"]) for s in samples if int(s["return_code"]) == 0]
    if not values:
        return {
            "count": 0,
            "mean": None,
            "median": None,
            "min": None,
            "max": None,
            "stdev": None,
        }
    return {
        "count": len(values),
        "mean": statistics.fmean(values),
        "median": statistics.median(values),
        "min": min(values),
        "max": max(values),
        "stdev": statistics.stdev(values) if len(values) > 1 else 0.0,
    }


def print_summary(summary: dict[str, float | int | None]) -> None:
    if summary["count"] == 0:
        print("summary: no successful samples")
        return
    print(
        "summary: "
        f"count={summary['count']} "
        f"mean={summary['mean']:.6f}s "
        f"median={summary['median']:.6f}s "
        f"min={summary['min']:.6f}s "
        f"max={summary['max']:.6f}s "
        f"stdev={summary['stdev']:.6f}s"
    )


def run_once(
    root: Path,
    args: argparse.Namespace,
    script: str,
    raw_log,
    run_items: list[tuple[bool, int]],
) -> tuple[list[dict[str, object]], float]:
    session = QemuSession(root, args, raw_log)
    session.start()
    assert session.boot_seconds is not None
    print(f"boot: {session.boot_seconds:.3f}s")
    results: list[dict[str, object]] = []
    try:
        for is_warmup, idx in run_items:
            label = f"{'warmup' if is_warmup else 'run'}-{idx}"
            result = session.run_script(script, label, args.command_timeout)
            result["warmup"] = is_warmup
            results.append(result)
            print(
                f"{label}: {result['elapsed_seconds']:.6f}s "
                f"rc={result['return_code']}"
            )
            if int(result["return_code"]) != 0 and not args.keep_going:
                raise SystemExit(f"{label} failed with rc={result['return_code']}")
    finally:
        session.stop()
    return results, session.boot_seconds


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Benchmark a command sequence inside CongCore/QEMU.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--script", help="guest shell script file to execute")
    parser.add_argument(
        "--cmd",
        action="append",
        help="guest shell command; may be passed more than once",
    )
    parser.add_argument("--repeat", type=int, default=1, help="measured iterations")
    parser.add_argument("--warmup", type=int, default=0, help="warmup iterations")
    parser.add_argument(
        "--reboot-each",
        action="store_true",
        help="boot a fresh guest for every iteration",
    )
    parser.add_argument("--arch", default="riscv64", choices=["riscv64", "loongarch64"])
    parser.add_argument("--smp", default="1")
    parser.add_argument("--mem", default="1G")
    parser.add_argument("--log", default="warn")
    parser.add_argument("--submit", default="0", choices=["0", "1"])
    parser.add_argument("--ext4-rebuild", default="0", choices=["0", "1"])
    parser.add_argument("--ext4-size", default="4G")
    parser.add_argument(
        "--make-var",
        action="append",
        default=[],
        metavar="NAME=VALUE",
        help="extra make variable appended to make run_ext4",
    )
    parser.add_argument(
        "--guest-shell",
        default="/musl/busybox sh",
        help="guest command that starts a POSIX-like shell reading stdin",
    )
    parser.add_argument("--guest-cwd", default="/musl")
    parser.add_argument("--no-set-e", dest="set_e", action="store_false")
    parser.set_defaults(set_e=True)
    parser.add_argument("--boot-timeout", type=float, default=180.0)
    parser.add_argument("--command-timeout", type=float, default=300.0)
    parser.add_argument("--prompt-timeout", type=float, default=30.0)
    parser.add_argument("--poweroff-timeout", type=float, default=30.0)
    parser.add_argument("--poweroff-cmd", default="poweroff")
    parser.add_argument("--keep-going", action="store_true")
    parser.add_argument("--verbose", action="store_true", help="tee guest output")
    parser.add_argument("--raw-log", help="write raw QEMU output to this file")
    parser.add_argument("--output", help="write JSON benchmark result")
    parser.add_argument(
        "--root",
        default=str(Path(__file__).resolve().parents[1]),
        help="CongCore workspace root to build and boot",
    )
    return parser


def main() -> int:
    args = build_parser().parse_args()
    if args.repeat < 1:
        raise SystemExit("--repeat must be >= 1")
    if args.warmup < 0:
        raise SystemExit("--warmup must be >= 0")

    root = Path(args.root).resolve()
    script = load_script(args)
    run_items = [(True, i + 1) for i in range(args.warmup)]
    run_items.extend((False, i + 1) for i in range(args.repeat))

    raw_log = open(args.raw_log, "ab") if args.raw_log else None
    all_results: list[dict[str, object]] = []
    boot_times: list[float] = []
    try:
        if args.reboot_each:
            for item in run_items:
                results, boot_seconds = run_once(root, args, script, raw_log, [item])
                all_results.extend(results)
                boot_times.append(boot_seconds)
        else:
            results, boot_seconds = run_once(root, args, script, raw_log, run_items)
            all_results.extend(results)
            boot_times.append(boot_seconds)
    finally:
        if raw_log:
            raw_log.close()

    measured = [r for r in all_results if not bool(r["warmup"])]
    summary = summarize(measured)
    print_summary(summary)

    payload = {
        "command_script": script,
        "config": {
            "arch": args.arch,
            "smp": args.smp,
            "mem": args.mem,
            "log": args.log,
            "submit": args.submit,
            "ext4_rebuild": args.ext4_rebuild,
            "ext4_size": args.ext4_size,
            "guest_shell": args.guest_shell,
            "guest_cwd": args.guest_cwd,
            "reboot_each": args.reboot_each,
        },
        "boot_seconds": boot_times,
        "runs": all_results,
        "summary": summary,
    }
    if args.output:
        Path(args.output).write_text(
            json.dumps(payload, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    return 0 if all(int(r["return_code"]) == 0 for r in all_results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
