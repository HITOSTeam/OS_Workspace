# Copilot Instructions for CongCore

## Project Overview

CongCore is a Rust-based OS kernel (rCore-style) targeting **riscv64** and **loongarch64**.

**Current primary goal: OS structural refactoring, code organization improvement, and bug fixing.**  
Fixes should be Linux-semantics-correct and reusable — not case-specific hacks. Actively reference open-source OS designs (Linux, seL4, Redox, etc.) when making architectural decisions.

## Repo Structure (Multi-Repo)

The workspace uses a root repo + two git submodules:

- **Root repo**: `user/`, `vendor/`, `ext4-fs/`, `ext4-fs-packer/`, `tools/`, top-level configs
- **`os/`** (submodule): kernel source — commit here for kernel changes
- **`OSGuide/`** (submodule): design docs, test progress, roadmap — commit here for doc changes

After committing to `os/` or `OSGuide/`, return to the root repo and update the submodule pointer.

## Key Commands

```sh
# Run full integration test; output goes to output.md
ARCH=riscv64 bash os/run.sh
ARCH=loongarch64 bash os/run.sh

# Type-check the kernel (use TMPDIR if /tmp is full)
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --manifest-path os/Cargo.toml --target riscv64gc-unknown-none-elf

# Find LTP tests not yet registered
python3 tools/find_unimplement_ltp.py --limit 60

# Scan output.md for failing LTP cases
python3 tools/find_ltp_error.py output.md

# Check all submodule/repo status
bash tools/status_all.sh
```

## Kernel Architecture (`os/src/`)

Initialization order in `main.rs`: `mm → log → trap → time → fs → task → scheduler`

| Module | Role |
|---|---|
| `task/processor.rs` + `task/manager.rs` | Per-hart processor + global task manager |
| `syscall/filesystem.rs` | Heavy syscall layer; ongoing refactor to push logic down into subsystems |
| `fs/` | `procfs`, `pipe`, `net_socket`, `socketpair`, `pseudo`, `mountns` |
| `mm/` | Page tables, COW, lazy fault, user copy |
| `ext4-fs/` | Block-layer filesystem (separate crate); currently guarded by a global `ext4_lock()` |

**Arch split**: `entry.asm` (riscv64) vs `entry_loongarch.S`. Constants split with `#[cfg(target_arch)]` throughout `config.rs`.

## Known Architecture Risks (Priority Order)

Consult `OSGuide/parts/architecture_improvement_roadmap.md` for full details. Top items:

1. **Global `ext4_lock()`** — serializes all filesystem ops; needs per-inode/per-dir lock granularity
2. **`/proc` pseudo-fs coupling** — procfs content still has ext4 inode dependencies; move to pure in-memory generation
3. **`syscall/filesystem.rs` monolith** — shared logic (path resolution, fd validation, umask) should move into subsystem helpers
4. **`exec` glibc magic-offset patch** — fragile private-layout hack; replace with proper ELF/auxv semantics
5. **Global task manager lock** — bottleneck under SMP; needs per-hart run queues with work stealing

## LTP & Testing Workflow

1. Pick a coherent batch (5–20 tests from one syscall family).
2. Implement fixes in `os/`, preferring shared mechanisms over per-case hacks.
3. Register new tests in `user/src/bin/ltp_dependence/` (`mod.rs` / `submit_plan.rs`).
4. Restore any `FOCUS_*` debug flags to `false` in `submit_plan.rs` before finishing.
5. Both **musl** and **glibc** paths must pass (`run_for_both_libcs()`).
6. Update `OSGuide/` docs after a batch passes.

The sdcard images (`sdcard-rv.img`, `sdcard-la.img`) are consumed by QEMU. **Back them up** — some LTP tests can corrupt them.

## Style & Conventions

- Rust 2024 edition, default `rustfmt`
- `snake_case` functions/modules, `CamelCase` types, `SCREAMING_SNAKE_CASE` constants
- Keep `unsafe` minimal; justify every `unsafe` block with a comment
- Use `read_user_cstring()` for user-pointer string reads — `translated_str()` kills the process on bad addresses and is incorrect
- Push shared fd/inode logic into `ProcessControlBlockInner` helpers, not duplicated per-syscall
- Return Linux-correct errno (`EFAULT`, `EINVAL`, `EXDEV`, …); wrong silent returns are bugs

## Commit Boundaries

- `os/` changes → commit inside `os/`
- `OSGuide/` changes → commit inside `OSGuide/`
- Everything else → root repo commit
- Use the same branch name across all three repos for the same task (e.g., `feat/ltp-semop-batch1`)
- Short imperative commit subject; include verification commands in body/PR notes

## Agent Workflow Guidelines

**Always use sub-agents to keep the main context lean.** The main context is only for decision-making, task decomposition, and synthesizing results.

### Standard Work Cycle

Repeat this cycle until a major milestone is complete, then **pause and report progress to the user**:

```
1. DISCOVER  →  2. PLAN & DISPATCH  →  3. WORK  →  4. REVIEW  →  loop
```

#### 1. Discover (explore agent)
Launch one or more `explore` agents in parallel to understand the current state:
- Audit code structure, find anti-patterns, measure file sizes
- Trace call chains or understand subsystem boundaries
- Research how reference OSes (Linux, arceos, starry-mix) handle the same problem

Never load large files into the main context. Always delegate exploration.

#### 2. Plan & Track (SQL todos)
Decompose findings into concrete todos. Record them in the SQL `todos` table with dependencies:
- Prefer small, independently verifiable tasks (one logical change per todo)
- **Do not assign a single agent an oversized task** — if a job touches > ~500 lines or > 3 files, split it into sequential sub-todos
- Use `todo_deps` to express ordering; maximize parallel execution

#### 3. Work (general-purpose agents, fleet mode)
Dispatch independent todos simultaneously as `general-purpose` background agents:
- Each agent gets a single, well-scoped task with explicit success criteria
- Each agent must run `cargo check` after changes and report 0 errors before finishing
- Each agent updates its todo status on completion (`UPDATE todos SET status = 'done' …`)
- After agents complete, query SQL for the next ready batch and dispatch immediately

#### 4. Review (code-review agent)
After each batch of work, launch a `code-review` agent on the uncommitted changes:
- The review agent checks only for real issues: bugs, incorrect semantics, missed cases, bad structure
- Treat 🔴 issues as new todos before proceeding; 🟡 issues are judgment calls
- Do not proceed to the next milestone until the review passes cleanly

### Sub-Agent Rules
- **Parallel by default**: dispatch all independent todos in one turn, not one at a time
- **Sync for quick tasks** (< 30s), **background for everything else**
- Never have one agent re-do what another already confirmed; trust SQL as source of truth
- If an agent fails or produces partial output, decompose further and retry

## Reference Docs

- `OSGuide/ltp_test_summary.md` — per-family test status
- `OSGuide/roadmap.md` — current phase and priorities
- `OSGuide/parts/architecture_improvement_roadmap.md` — subsystem risk list and governance work
