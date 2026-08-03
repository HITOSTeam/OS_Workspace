# 2026-08-03 LoongArch lazy fault 本地 TLB 发布修复

## 1. 结论

本批把 LoongArch lazy fault 的 `non-present -> present` PTE 发布从同步 mm 级
shootdown 改为 faulting hart 本地、指定 ASID、8 KiB pair 范围的失效。实现边界参考
Linux：首次安装缺失 PTE 不向其他 CPU 执行 `flush_tlb_page()`；并发 CPU 如果保留
invalid translation，会 fault 后重读已经有效的 PTE，并只刷新自己的 MMU cache。

以下仍保留原有同步跨 hart invalidation：

- COW 替换物理页；
- `mprotect` 权限修改；
- `munmap`、回收和旧 frame 延迟释放；
- supervisor-only trap-context PTE 发布；
- 共享 kernel page table 更新。

RISC-V 路径没有在本批改变，继续使用原有 PTE 发布 transaction 与 I-cache 同步。

## 2. 环境与版本

| 项目 | 值 |
| --- | --- |
| 工作树 | `.tmp/worktrees/loongarch-linux-fix` |
| 分支 | `loongarch-linux-fix` |
| 修改前上层 commit | `7f391ea16e4a4bdd9aab39485328535014053ada` |
| 修改前 `os` commit | `54d0a199878bd81e32a9aa5bb4382ce888b2a1cd` |
| 内核修复 commit | `35bf4d34bd46ebde71ff8ddc4ae5358421db502b` |
| 回归测试 commit | `41e068a6ed32018180d7301a2b24240a988619cb` |
| `os` 指针 commit | `75e17fb4b67b5c22ace2dc57134a84eaa7b824c6` |
| final 测例分支/commit | `final-2026` / `1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36` |
| LoongArch 镜像 | `sdcard-la-pub.img` |
| 镜像 SHA-256 | `2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a` |
| QEMU | `11.0.3` |
| 运行配置 | `loongarch64`, 12 vCPU, 8 GiB, `-snapshot` |
| Linux 参考 commit | `fc02acf6ac0ccde0c805c2daa9148683cdd01ba8` |

镜像继续以 `-snapshot` 只读基准方式使用，没有修改或替换 raw 基准镜像。checksum
沿用同一批次开始时已验证的不可变资产记录。

## 3. 失败现象与根因

前一批 120 秒 `tg-xtask` 聚焦窗口记录到：

- `tlb_page_batches` 约 313135；
- `tlb_range_batches` 785；
- `tlb_asid_drops` 59；
- `tlb_remote_ipis` 5047；
- `tlb_shootdown_wait_cycles` 13925600。

普通文件 mmap 改为 lazy/page-cache 后，每个首次访问页都会进入
`MemorySet::commit_lazy_fault()`。旧代码即使确认 leaf PTE 原先无效，也会：

1. 开启 `PageTableUpdateBatch` 和 mm invalidation sequence；
2. 安装新 PTE；
3. `record_page(fault_va)`；
4. 枚举该 mm 的 active/current-hart ASID，必要时发送 IPI 并等待 ack。

这对“旧 translation 仍可能访问错误物理页”的更新是必需的，但首次 missing PTE
没有旧的有效 translation。远端最多保留 LoongArch paired TLB 的 invalid half；它
可以在自己真正访问该地址时，通过正常用户缺页路径恢复，不需要发布者提前停止所有
CPU。

## 4. Linux 对照

本批对照本地 Linux 源码以下路径：

- `mm/memory.c` 的 missing-PTE fault 完成路径明确区分“原先 non-present”，安装后
  调用 `update_mmu_cache_range()`，不执行通用 TLB invalidation；
- `arch/loongarch/include/asm/pgtable.h` 的 `update_mmu_cache_range()` 逐页进入
  `__update_tlb()`；
- `arch/loongarch/mm/tlb.c::__update_tlb()` 只更新当前 CPU；启用 hardware PTW 时
  直接返回；
- `arch/loongarch/mm/tlb.c::local_flush_tlb_page()` 按 `PAGE_SIZE << 1` 对齐
  LoongArch paired entry；
- `mm/memory.c::handle_pte_fault()` 把 racing fault 视为可恢复的 spurious fault；
  protection fault/旧权限场景仍使用更强的 flush 规则。

CongCore 使用 QEMU LoongArch paired TLB，且曾验证过相邻有效页可能留下 invalid
half，因此没有把 `update_mmu_cache()` 简化成无条件 no-op，而是保留一次当前 hart
的 pair-local `invtlb`。

## 5. 修复设计

### 5.1 首次 PTE 发布

新增 `update_mmu_cache_for_new_pte()`：

1. 读取当前 hart 在该 mm 中缓存的 `(generation, ASID)`；
2. 如果本 hart 没有可复用 context，则不失效；后续 `prepare_user_asid()` 会分配
   干净 context；
3. 如果 context 有效，先用 `dbar 0` 发布 PTE store；
4. 对 fault VA 所在的 8 KiB pair 执行当前 hart、指定 ASID 的 `invtlb`；
5. 再用 `dbar 0` 把失效操作排在返回用户态之前。

该路径不进入 mm invalidation sequence，不扫描 active hart mask，不发送 IPI，也
不等待远端 ack。

### 5.2 并发 fault 恢复

另一个 hart 可能已经缓存同一 pair 的 invalid half。它访问该页时，
`prepare_lazy_fault()` 会发现 PTE 已有效并返回 `Resolved`。LoongArch 现在只调用
同一个 `update_mmu_cache_for_new_pte()` 刷新自己的 pair，然后重试用户指令。

### 5.3 保留强同步的边界

`commit_cow_fault()`、权限更新、unmap、frame retirement 与 `MapArea::map_batched()`
没有改成 local-only。这些路径可能存在有效旧 translation，或像 trap-context
一样无法依赖普通用户缺页恢复，必须继续等待目标 hart 完成 invalidation 后才能
发布权限或复用物理页。

## 6. 新增回归

新增 `lazy_fault_local_tlb_smoke`，使用共享 mm 的两个线程并分别固定到 CPU 0/1：

1. 建立 512 页只读 `MAP_PRIVATE` 文件映射；
2. CPU 0 先访问每个 8 KiB pair 的偶页，使本地 pair 可能保留无效奇页；
3. CPU 1 访问并发布全部奇页 PTE，不要求 shootdown CPU 0；
4. CPU 0 再访问奇页，必须通过 spurious fault + 本地 pair refresh 恢复；
5. 校验全部读取内容，并清理映射。

该测试同时覆盖普通文件 page cache lazy fault、共享 mm、CPU affinity、paired TLB
negative half 和远端 PTE 发布后的本地恢复。

## 7. 验证结果

### 7.1 静态检查

以下命令均返回 0：

```bash
TMPDIR=$PWD/.tmp cargo check -q --manifest-path os/Cargo.toml --target loongarch64-unknown-none
TMPDIR=$PWD/.tmp cargo check -q --manifest-path os/Cargo.toml --target riscv64gc-unknown-none-elf
TMPDIR=$PWD/.tmp cargo check -q --manifest-path user/Cargo.toml --bin lazy_fault_local_tlb_smoke --target loongarch64-unknown-none
TMPDIR=$PWD/.tmp cargo check -q --manifest-path user/Cargo.toml --bin lazy_fault_local_tlb_smoke --target riscv64gc-unknown-none-elf
rustfmt --edition 2024 --check \
  os/src/arch/loongarch64/mm/asid.rs \
  os/src/mm/memory_set/fault.rs \
  os/src/mm/memory_set/mm_ref.rs \
  user/src/bin/smoke_archive/lazy_fault_local_tlb_smoke.rs
git diff --check
git -C os diff --check
```

编译输出仍包含仓库原有 warnings，本批没有新增编译错误。

### 7.2 LoongArch 聚焦运行

启动命令：

```bash
make -C os run_final \
  ARCH=loongarch64 SUBMIT=0 BASH_SHELL=1 LOG=warn \
  SMP=12 MEM=8G EXT4_REBUILD=0 USER_EXT4_SIZE=256M \
  FINAL_IMG=/Users/bytedance/projects/OS_Workspace/testsuits-final/sdcard-la-pub.img \
  QEMU_TIMEOUT=0 QEMU_EXTRA_ARGS=-snapshot
```

guest 中结果：

```text
lazy_fault_local_tlb_smoke passed
private_file_page_cache_smoke passed
tlb_shootdown_smp_smoke passed
```

语义日志：

`testsuits-final/.tmp/final-runs/20260803-lazy-local-tlb/serial.log`

### 7.3 Perf 计数

临时设置 `DEBUG_PERF=true`，只运行新回归并读取前后 `/proc/perf`；采集后已恢复为
`false`，该临时开关未进入提交。

| 计数 | 运行前 | 运行后 | 增量 |
| --- | ---: | ---: | ---: |
| `tlb_page_batches` | 62 | 165 | 103 |
| `tlb_range_batches` | 23 | 41 | 18 |
| `tlb_asid_drops` | 5 | 7 | 2 |
| `tlb_batched_edits` | 1700 | 2483 | 783 |
| `tlb_merged_ranges` | 85 | 206 | 121 |
| `tlb_exact_pairs` | 682 | 1821 | 1139 |
| `tlb_remote_ipis` | 88 | 154 | 66 |
| `tlb_shootdown_wait_cycles` | 251400 | 411600 | 160200 |

回归本身执行 512 次首次 file-PTE 发布，并让 CPU 0 再访问 256 个由 CPU 1 发布的
奇页。这些 fault 动作不再一页对应一个 mm page batch；增加的本地 pair 操作计入
`tlb_exact_pairs`。仍有的 batch/IPI 来自 exec、clone、trap-context、映射销毁和
线程退出等生命周期更新，本批有意保留其同步语义。

Perf 日志：

`testsuits-final/.tmp/final-runs/20260803-lazy-local-tlb-perf/serial.log`

## 8. 未覆盖范围与后续工作

- 未运行完整 BuildStorm，遵守本轮聚焦验证约束；
- 未运行完整初赛/LTP；
- RISC-V 只做编译检查，没有运行 QEMU runtime；
- 未对修改前内核重跑同一 smoke 做严格 A/B；本报告保留了修改后的 counter delta，
  并引用前一批真实 `tg-xtask` 的高 page-batch 现场作为根因证据；
- 下一次 BuildStorm 聚焦窗口应确认 `tg-xtask` 的 `tlb_page_batches` 与 remote IPI
  明显下降，再继续分析 rustc 剩余的文件系统、调度或内存分配瓶颈。

## 9. AI 使用说明

本批使用 AI 辅助完成本地 Linux 源码与 CongCore fault/TLB 路径的对照、并发边界
推演、代码修改、回归设计、命令执行和报告整理。所有结论均以本地源码、串口日志、
实际编译结果和 `/proc/perf` 计数复核；没有伪造测试输出、计时或 `/proc/uptime`。
