# Repository Guidelines

## Project Structure & Module Organization

- `Cargo.toml` defines a Rust workspace containing `os/`, `user/`, `tests/`, `easy-fs/`, and `easy-fs-fuse/`.
- `os/`: kernel sources and QEMU-related targets (see `os/Makefile`).
- `user/`: user-space apps compiled into `os/results/*.bin`.
- `ext4-fs/` and `ext4-fs-packer/`: ext4 implementation and image builder; `ext4-fs-packer/extra/` contains test assets.
- `tests/`: Rust tests for kernel components.
- `testsuits-for-oskernel/`: external OS test suites.
- `exampleOs/`: reference OS implementations for comparison and debugging ideas.
- `vendor/`: pinned third-party crates (for example, `smoltcp` and `virtio-drivers`).

## exampleOs

Use `exampleOs/` as a reference when debugging issues or adding features.

## Build, Test, and Development Commands

At this stage, the main workflow is running tests and fixing kernel issues.

```zsh
ARCH=riscv64 bash os/run.sh
ARCH=loongarch64 bash os/run.sh
```

### `os/run.sh` notes

- `SUBMIT=1` runs `submit_script.rs` for the selected test file in `user/src/bin`.
- `ARCH` selects the target architecture (`riscv64` or `loongarch64`).
- For target details, check `os/Makefile`; `run_ext4` is the primary target.
- After running, review `output.md` (it can be large, so read selectively).
- Use tools under `tools/` to locate and analyze errors.

### Tools

- `tools/find_ltp_error.py`: Scans test logs (file path argument or stdin) and prints abnormal LTP results, including non-zero `FAIL LTP CASE ... : N` items and lines containing `TFAIL`/`TBROK`/`TSKIP`.
- `tools/find_unimplement_ltp.py`: Lists LTP tests in `ltp_all.md` that are not covered by `user/src/bin/ltp_dependence/mod.rs`. Consecutive tests sharing the same prefix are compressed into range notation (e.g. `semop[01-05]`). Optional flags: `--output` to write the raw (uncompressed) missing list to a file, `--limit` to cap stdout output, and `--no-compress` to print every test name individually.

### Notes for work

Now we are working on LTP tests. Ltp tests are listed in `ltp_all.md` (large file, you should read partially).
When implementing the OS, you should not only make it pass the LTP tests, but also meet the standard of an OS like Linux.
Tell me to do `git commit` when enough work has been done, and provide the commit's content.

#### Recommended workflow for each session

1. **Read the summary first.**
   Open `OSGuide/ltp_test_summary.md` to understand how the ~2800 tests are
   grouped into categories and which categories are highest priority.

2. **Find uncovered tests.**
   Run `python3 tools/find_unimplement_ltp.py --limit 60` to get a compressed
   list of tests not yet registered in `user/src/bin/ltp_dependence/mod.rs`.
   Use `--no-compress` or `--output missing.txt` if you need the raw list.

3. **Pick a focused batch.**
   Choose a coherent group of 5–20 tests from one category (preferably a
   high-priority category from the summary). Prefer groups that share the
   same underlying syscall family so fixes compose well.

4. **Implement and register.**
   - Add kernel-side fixes in `os/`.
   - Register the new tests in the appropriate `pub const *_TASKS` array in
     `user/src/bin/ltp_dependence/mod.rs`, and wire them into `submit_plan.rs`
     if needed.

5. **Verify.**
   Run `ARCH=riscv64 bash os/run.sh`, then use `tools/find_ltp_error.py` on
   `output.md` to confirm the targeted tests pass and no regressions appear.

6. **Mark progress in the summary.**
   After the batch passes, add a ✅ marker next to the relevant group in
   `OSGuide/ltp_test_summary.md`, e.g.:
   ```
   | `semop` | semop[01-05] | ✅ |
   ```
   Also update the **Coverage Status** table at the bottom of that file.

#### Long-term Working Policy

- Do not choose between "only passing LTP" and "only refactoring architecture first". Use **LTP-driven development with architecture improvement in parallel**.
- Recommended pace: roughly **70% LTP progression + 30% architecture/governance improvements**.
- For each failing test group, prefer reusable Linux-like semantic fixes, not case-specific hacks.
- Any fix that risks long-term maintainability should be converted into a proper subsystem improvement while solving that test.
- Goal: keep continuous LTP progress while steadily moving toward a clean, real-OS-style design.
- For medium/long-term architecture work, use `OSGuide/parts/architecture_improvement_roadmap.md` as the baseline roadmap and keep it updated after major fixes.

## Coding Style & Naming Conventions

- Rust 2024 edition is used in `os/` and `user/`.
- No repo-specific formatter config is provided; use default `rustfmt`.
- Naming conventions:
  - `snake_case` for modules and functions.
  - `CamelCase` for types.
  - `SCREAMING_SNAKE_CASE` for constants.
- Keep `unsafe` blocks minimal and clearly justified (kernel code is sensitive).

## Commit & Pull Request Guidelines

- This checkout does not include full git history, so no strict local convention is visible.
- Prefer short, imperative commit subjects with context (area, intent, risk).
- PRs should include:
  - A short summary.
  - Commands used for testing (or why tests were not run).
  - QEMU logs/screenshots when behavior changes.
- To inspect git history, run git commands in the relevant subdirectory (for example, `os/` or `user/`).
