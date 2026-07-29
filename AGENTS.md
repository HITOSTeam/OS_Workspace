# Repository Guidelines

## Focus

- This project is in the **LTP-driven development + architecture improvement** stage.
- Do not optimize only for "one more test passes". Prefer Linux-like semantics, reusable fixes, and maintainable subsystem changes.
- Later contributors are expected to keep optimizing, refactoring, and improving transitional implementations.

## Project Structure

- `os/`: kernel source and QEMU targets.
- `user/`: user-space apps and LTP submit wiring.
- `testsuits-for-oskernel/`: external test suites.
- `ext4-fs/`, `ext4-fs-packer/`: filesystem implementation and image builder.
- `exampleOs/`: reference implementations for debugging ideas.

## Core Workflow

1. Pick a coherent batch, usually 5 to 20 tests from one syscall family or object type.
2. Implement kernel-side fixes in `os/`, preferring shared mechanisms over case-specific hacks.
3. Register tests in `user/src/bin/ltp_dependence/` when needed.
4. Verify with focused regression first.
5. Update docs after the batch passes.
6. Restore any temporary debug narrowing in `submit_plan.rs` before finishing.

## Commands

```zsh
ARCH=riscv64 bash os/run.sh
ARCH=loongarch64 bash os/run.sh
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --manifest-path os/Cargo.toml --target riscv64gc-unknown-none-elf
python3 tools/find_unimplement_ltp.py --limit 60
python3 tools/find_ltp_error.py output.md
```

- If `/tmp` is full, use `TMPDIR=$PWD/.tmp`.
- `os/run.sh` writes the main log to `output.md`.

## Style

- Rust 2024 edition, default `rustfmt`.
- `snake_case` for functions/modules, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for constants.
- Keep `unsafe` minimal and justified.

## Commit Notes

- When a coherent batch is done, propose a commit scope and message.
- Prefer short imperative commit subjects.
- Include verification commands in commit or PR notes.
