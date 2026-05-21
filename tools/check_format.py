#!/usr/bin/env python3
"""Check repository text formatting rules used by pre-commit."""

from __future__ import annotations

import sys
from pathlib import Path


MARKDOWN_SUFFIXES = {".md", ".markdown"}


def check_file(path: Path) -> list[str]:
    if not path.exists() or not path.is_file():
        return []

    data = path.read_bytes()
    if not data or b"\0" in data:
        return []

    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as err:
        return [f"{path}: not valid UTF-8 ({err})"]

    errors: list[str] = []

    if b"\r" in data:
        errors.append(f"{path}: contains CRLF/CR line endings; use LF")

    if not data.endswith(b"\n"):
        errors.append(f"{path}: missing final newline")

    if path.suffix.lower() not in MARKDOWN_SUFFIXES:
        for line_number, line in enumerate(text.splitlines(keepends=True), start=1):
            body = line[:-1] if line.endswith("\n") else line
            if body.endswith((" ", "\t")):
                errors.append(f"{path}:{line_number}: trailing whitespace")
                break

    return errors


def main(argv: list[str]) -> int:
    errors: list[str] = []
    for raw_path in argv[1:]:
        errors.extend(check_file(Path(raw_path)))

    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
