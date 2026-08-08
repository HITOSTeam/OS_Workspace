# 2026-08-07 RISC-V missing-PTE 本地 TLB 发布修复

## 问题概述

RISC-V 每次把无效用户 PTE 变成有效 PTE，都会刷新其他核的 TLB。BuildStorm
并发编译时，新页很多，同一地址空间又会在多个核上运行。这条远程刷新路径因此反复
触发。Linux 对这种更新只刷新发生缺页的本核；CongCore 改成相同做法。旧映射失效、
换页、降权、解除映射、COW 和可执行页发布仍通知其他核。

## 背景知识

TLB 就是页表的缓存。可以把页表看成地址簿，把 TLB（Translation Lookaside Buffer，
地址转换旁路缓存）看成手边的常用联系人。CPU 先查 TLB；没找到时才逐级查页表。

Sv39 是 RISC-V 的 39 位虚拟地址分页方案。它把地址分成三段页表索引和页内偏移：

```text
虚拟地址低 39 位
+----------+----------+----------+-------------+
| VPN[2]   | VPN[1]   | VPN[0]   | page offset |
| 9 bit    | 9 bit    | 9 bit    | 12 bit      |
+-----+----+-----+----+-----+----+-------------+
      |          |          |
      v          v          v
   一级页表 -> 二级页表 -> 三级页表 -> 4 KiB 物理页
```

VPN（Virtual Page Number，虚拟页号）每段选择 512 个表项之一。最后得到 PTE
（Page Table Entry，页表项）。PTE 的 V 位表示有效；V=0 时没有映射。R、W、X
分别允许读、写、执行，U 表示用户态可访问。A、D 还记录访问和写入状态。

CPU 可能把“这个 PTE 无效”也记进 TLB。因此页表内存改好后，要用 `sfence.vma`
（Supervisor Virtual Memory Fence，监管态虚拟内存屏障）让旧缓存不能继续生效。
它有四种常用形式：

```text
sfence.vma x0,   x0     刷本核全部地址、全部 ASID
sfence.vma addr, x0     刷本核某个虚拟地址
sfence.vma x0,   asid   刷本核某个 ASID 的全部地址
sfence.vma addr, asid   刷本核某个 ASID 的某个地址
```

ASID（Address Space Identifier，地址空间标识符）像贴在 TLB 项上的进程标签。
不同进程可把相同虚拟地址映到不同物理页。只要标签不同，切进程时就不用清空整个
TLB。CongCore 还给 ASID 加代际，避免编号回收后误用旧缓存。

`sfence.vma` 只管当前 hart（硬件线程，本文可近似看作一个 CPU 核）。RISC-V
没有替操作系统广播 TLB 刷新的硬件指令。需要刷其他核时，内核调用 SBI
（Supervisor Binary Interface，监管态二进制接口）的 remote fence（远程屏障）
服务。固件再发 IPI（Inter-Processor Interrupt，核间中断），等待目标核执行屏障。
这会跨核、跨特权级同步，代价远高于一次本地指令。

无效 PTE 变成有效 PTE 时，其他核没有旧物理页或旧权限可误用。发生缺页的核刷自己
即可。其他核若缓存了“无效”，最多再缺页一次，然后也在本地刷新。反过来，有效 PTE
变成无效，或改成另一物理页、收紧权限时，其他核可能继续使用旧映射，必须全部通知。

取指还多一层缓存。I-cache（Instruction Cache，指令缓存）可能留着旧指令；
`fence.i`（取指屏障）让本核之后重新取指。可执行页可能被别的核运行，所以本批没有
把它放进数据页快路，仍保留跨核 TLB 与 I-cache 同步。

## 如何发现

长测中的 shell 探针一直成功，QEMU 也持续占用 CPU，所以先排除了死锁和 OOM。
随后用 QEMU `-perfmap` 定位：单核差距不大，多核差距放大到 4--5 倍。源码追踪显示，
两个新 PTE 路径都进入 SBI RFENCE。原始日志、命令和调用点完整保存在下文。

## 怎么解决

新增 `update_mmu_cache_for_new_pte()`。它读取本核当前代际的 ASID，在 PTE 写入屏障后，
只执行该地址和该 ASID 的本地 `SFENCE.VMA`。普通数据页和并发伪缺页走这条路径；
可执行页及所有曾经有效的 PTE 仍走原有的跨核事务。

## 对应提交

内核修复为 `ff9c87df468a025dddc087bad28937032a22c80b`，提交主题是
`riscv: avoid remote shootdown for new PTEs`。顶层基线、内核基线、Linux 对照提交和
文件清单见历史记录。

## 对比提升

在 8 vCPU、8 GiB、300 秒 ABBA 对照中，14 个可比采样的编译输出平均增加 18.77%。
两轮 after 都推进到更多 crate，QEMU CPU 只增加约 0.5%。6 项 TLB、文件映射、共享
地址空间和 I-cache 回归为 6 passed / 0 failed。四轮都因 300 秒上限以 `rc=124`
结束，所以这证明短窗口吞吐提升，不代表完整 BuildStorm 已通过。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

# 2026-08-07 RISC-V missing-PTE 本地 TLB 发布修复

## 问题概述

RISC-V BuildStorm 的 `tg-xtask` 前置编译并没有死锁。旧实现的问题是：每次 lazy
fault 把一个原先不存在的用户 PTE 安装为有效 PTE 时，都会开启完整的 mm 失效事务，
扫描 resident harts，并通过 SBI RFENCE 同步刷新其他 hart 的 TLB。

这类更新没有需要从远端删除的旧有效 translation。多线程 rustc 让同一 mm 驻留在
多个 hart 后，原本应为本地操作的每个数据页首次缺页都会变成跨特权级、跨 hart 的
同步请求，RISC-V 的吞吐因而明显低于已经修过同类问题的 LoongArch。

本批按 Linux 的 missing-PTE 语义，把 RISC-V 非可执行新 PTE 发布改为 faulting hart
上的 `SFENCE.VMA address, asid`。旧有效 PTE 的替换、降权、unmap、COW 和可执行页
发布仍保留同步失效，不能使用这个快路。

## 如何发现

### 1. 先用活性和资源证据排除死锁

原始长测：

```text
testsuits-final/.tmp/final-runs/20260807-buildstorm-riscv-full-128/run/
testsuits-final/.tmp/final-runs/20260807-riscv-resume-liveness-verify/run/
```

`20260807-buildstorm-riscv-full-128` 中 48 次 shell 探针全部成功，延迟为
738--8126 ms；探针能够持续 fork/exec BusyBox、读取 `/proc/uptime` 和输出文件。
串口日志没有 panic、OOM 或 deadlock。输出不增长时 QEMU 仍持续使用约两个 host
CPU，且 LoongArch 已成功的运行也出现过相同形状的数分钟编译静默期。因此该现场是
rustc 正在进行低日志输出的计算阶段，不是整个 guest 卡死。

这轮使用的 4200 秒总上限也不能作为官方失败线。`testsuits-final/run.sh` 对
BuildStorm 的默认 `TEST_TIMEOUT` 是 18000 秒；短窗口只能用于定位和 A/B，不能写成
完整 BuildStorm 结果。

### 2. 用 perf 定位多核 RISC-V 特有放大器

QEMU TCG JIT 代码在普通 `perf` 报告中会聚成一个匿名代码块。QEMU 官方文档推荐：

```sh
perf record -- qemu-system-riscv64 -perfmap ...
perf report
```

`-perfmap` 是轻量的 guest-host 映射，适合把 host PC 重新归因到 guest 代码。它会改变
TCG 运行条件，所以本批只在定位样本中启用；严格计时 A/B 两边都关闭 `perfmap`。

定位结果排除了以下主因：QEMU `qemu_cpu_kick`、普通 IPI handler、调度负载不均、
持自旋锁被抢占和内核堆 OOM。单线程微基准没有远端 hart，RISC-V/LoongArch 差距约
1.4 倍；同一 workload 扩展到多 hart 后差距放大到 4--5 倍，指向 mm 的同步跨核路径。

源码证据闭合在两个调用点：

- `os/src/mm/memory_set/fault.rs::commit_lazy_fault()`：普通 lazy fault 安装 PTE 后，
  RISC-V 进入 `begin_page_table_update()`；
- `os/src/mm/memory_set/mm_ref.rs::refresh_new_pte_fault()`：并发线程发现 PTE 已由另一
  线程安装时，RISC-V 调用 `flush_user_page()`。

两者最终都进入 `begin_user_tlb_batch()`、`shootdown_range()` 和 SBI RFENCE。缺页越多、
mm 驻留 hart 越多，等待成本越高。

## Linux 对照和安全边界

本批对照本地 Linux `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`：

- `mm/memory.c` 的 missing-PTE 完成路径明确写着
  `No need to invalidate - it was non-present before`，随后调用
  `update_mmu_cache_range()`；
- `arch/riscv/include/asm/pgtable.h::update_mmu_cache_range()` 不做远端 shootdown，
  只逐页调用 `local_flush_tlb_page()`；
- RISC-V 允许微架构缓存 invalid PTE，因此没有 Svvptc 时仍需要本地
  `SFENCE.VMA`；其他 hart 若保留 invalid entry，最多产生一次可恢复的 spurious
  fault，不会看到旧物理页或旧权限。

CongCore 的安全前提与 Linux 一致：

1. `commit_lazy_fault()` 在安装前重新检查 VMA 和 PTE；只有确认 PTE 仍无效才走新页
   快路；
2. 并发 hart fault 时，`prepare_lazy_fault()` 发现 `pte.is_valid()` 后返回
   `Resolved`，再在当前 hart 刷新该地址；
3. COW、`mprotect`、`munmap`、frame retirement 等存在旧有效 translation 的路径没有
   改动；
4. 带 `PTEFlags::X` 的新映射仍使用原有 mm-wide TLB transaction 和 I-cache
   同步。数据页快路不能绕过跨 hart `fence.i`。

## 怎么解决

### 1. 新增 RISC-V new-PTE MMU-cache helper

`os/src/arch/riscv64/mm/asid.rs` 新增
`update_mmu_cache_for_new_pte()`：

1. 读取当前 hart 实际加载的 generation-tagged ASID；
2. 若没有当前 generation 的可复用用户 context，则返回；后续
   `prepare_user_satp()` 会安装干净 context，ASID 关闭时会执行完整本地 flush；
3. 用现有 `page_table_write_barrier()` 把 PTE store 排在 fence 之前；
4. 只执行当前 hart、当前 ASID、fault page 的 `SFENCE.VMA`；
5. 不进入 `invalidation_sequence`，不扫描 resident hart mask，不发 SBI RFENCE，也不
   等远端确认。

### 2. 拆分数据页和可执行页提交

`os/src/mm/memory_set/fault.rs` 只在 `PTEFlags::X` 时创建原有
`PageTableUpdateBatch`。非 X PTE 安装后调用新 helper；X PTE 继续 record page、标记
I-cache stale 并提交完整事务。

### 3. 修复并发 spurious-fault 路径

`os/src/mm/memory_set/mm_ref.rs` 的 RISC-V
`refresh_new_pte_fault()` 改用同一个本地 helper。这样远端 hart 因缓存 invalid entry
再次 fault 时，只刷新自己，不会把一次可恢复竞争重新放大成第二轮跨核 shootdown。

## 对应提交

内核修复已单独提交；顶层集成提交只更新 `os` 指针并加入本报告：

| 项目 | 值 |
| --- | --- |
| 顶层基线 | `77a1fb7696f44429740c28ed4ab82892624783ee` |
| `os` 基线 | `62537ad5474aa1dbffded6a2be88324910d7d43d` |
| 内核修复 | `ff9c87df468a025dddc087bad28937032a22c80b`（`riscv: avoid remote shootdown for new PTEs`） |
| 顶层集成 | 本报告所在提交（`riscv: integrate new-PTE local TLB update`） |
| 涉及文件 | `asid.rs`、`fault.rs`、`mm_ref.rs` |

当前工作树还有其他工作人员的未提交修改。本批使用精确 index patch 拆开
`fault.rs` 的重叠 hunk，没有把其中的 shared-backing 修改或其他脏文件混入提交。

## 修改前后的提升对比

### 受控方法

- 从同一个私有源码快照构建 before/after；before 只反转本批 new-PTE 修改，其他工作树
  内容相同；
- 四轮都使用 8 vCPU、8 GiB、同一个只读 raw 基准镜像，各自创建全新 qcow2 child
  overlay；
- 四轮使用同一个私有 `user-riscv.ext4`；
- 每轮执行 `cargo build -p tg-xtask`，guest 硬上限均为 300 秒；
- 使用 `before -> after -> after -> before` 的 ABBA 顺序，抵消宿主页缓存和运行顺序
  影响；
- 四轮都关闭 `DEBUG_PERF`、`DEBUG_SCHED` 和 QEMU `-perfmap`；
- 每约 30 秒运行活性探针，并以 host metrics 连续记录 QEMU CPU、RSS 和 I/O；
- 到 300 秒立即终止，不把预计会继续运行的前置编译留在后台。

受控内核：

| 版本 | SHA-256 |
| --- | --- |
| before | `00cdfca6a9322a99b1119483daac8cb35527d9ab1e5ee22f0e5b3d1274b0ed0d` |
| after | `25c037b875b9c3d7e65da64ea2117165ce773beff3270e51658460237d3d8736` |

原始数据：

```text
testsuits-final/.tmp/final-runs/20260807-riscv-new-pte-tg-before-152/run/
testsuits-final/.tmp/final-runs/20260807-riscv-new-pte-tg-after-153/run/
testsuits-final/.tmp/final-runs/20260807-riscv-new-pte-tg-reverse-after-154/run/
testsuits-final/.tmp/final-runs/20260807-riscv-new-pte-tg-reverse-before-155/run/
```

### 300 秒前置编译吞吐

第一对 after 的采样总比对应 before 早 1.09--3.13 秒，因此下表的领先不是多运行时间
造成的：

| 采样 | before：uptime / output bytes | after：uptime / output bytes | output 增幅 |
| ---: | ---: | ---: | ---: |
| 3 | 91.84 s / 606 | 90.75 s / 742 | +22.44% |
| 4 | 122.20 s / 797 | 120.04 s / 1051 | +31.87% |
| 5 | 152.76 s / 1051 | 150.27 s / 1149 | +9.32% |
| 6 | 183.10 s / 1107 | 180.50 s / 1267 | +14.45% |
| 7 | 213.65 s / 1232 | 210.81 s / 1444 | +17.21% |
| 8 | 244.06 s / 1380 | 241.07 s / 1744 | +26.38% |
| 9 | 274.49 s / 1600 | 271.36 s / 1981 | +23.81% |

反向顺序的第二对得到同一方向的结果：第 3--9 个采样平均增幅为 **16.75%**；273.48
秒时 after 为 1981 B，274.01 秒时 before 仅为 1623 B，即 after 在更短时间仍领先
22.06%。两对共 14 个第 3--9 采样的平均增幅为 **18.77%**，排除了“after 只是后跑，
命中宿主页缓存”的解释。

日志字节不是完成时间本身，所以同时核对了 crate 进度：两个 before 的最终 tail 最远
只到 `serde_json`/`ryu`；两个 after 都已继续启动 `zerocopy`、`getrandom`、`ring`、
`aws-lc-rs` 等后续 crate。该结果证明同一 300 秒窗口内完成了更多真实编译工作。

资源数据也与吞吐提升一致：

| 指标 | before | after | 解释 |
| --- | ---: | ---: | --- |
| 两轮平均 guest elapsed | 300.19 s | 300.25 s | 时间窗口相同 |
| 两轮平均 QEMU CPU | 7.388 核 | 7.425 核 | 只增加约 0.5%，不是 18.77% 进度差来源 |
| 两轮平均 process write bytes | 206.4 MiB | 245.7 MiB | after 产生约 19.0% 更多编译输出 |
| 最后可比探针 output 均值 | 1611.5 B | 1981 B | after 领先 22.93% |
| 两对探针延迟中位数 | 432 / 330 ms | 266 / 261 ms | guest 仍可响应，无停滞恶化 |

四轮都按设计以 `rc=124` 结束，`tg-xtask` 在 300 秒内尚未生成；这不是完整
BuildStorm 通过。四份 qcow2 经 `qemu-img check` 均无错误，串口均无 panic/OOM。

## 回归验证

### 静态构建

以下检查通过，只有仓库已有 warnings：

```sh
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check \
  --manifest-path os/Cargo.toml --target riscv64gc-unknown-none-elf
TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check \
  --manifest-path os/Cargo.toml --target loongarch64-unknown-none-softfloat
rustfmt --edition 2024 --check \
  os/src/arch/riscv64/mm/asid.rs \
  os/src/mm/memory_set/fault.rs \
  os/src/mm/memory_set/mm_ref.rs
git -C os diff --check
```

### RISC-V 运行态

运行目录：

```text
testsuits-final/.tmp/final-runs/20260807-riscv-new-pte-regressions-after-147/
```

| 测试 | 结果 | 覆盖边界 |
| --- | ---: | --- |
| `lazy_fault_local_tlb_smoke` | PASS | 两 hart missing-PTE 与 spurious fault 恢复 |
| `file_mmap_lazy_fault_smoke` | PASS | 普通文件 lazy fault |
| `private_file_page_cache_smoke` | PASS | private file page-cache 映射 |
| `clone_vm_mmap_smoke` | PASS | 共享 mm 并发访问 |
| `tlb_shootdown_smp_smoke` | PASS | 旧有效 PTE 仍执行跨核失效 |
| `riscv_icache_smp_smoke` | PASS | 可执行页仍同步 I-cache |

结果为 **6 passed / 0 failed**，每项都有 60 秒硬上限；串口无 panic/OOM，测试结束后
没有遗留 QEMU。

## 当前结论和后续

这轮已经证明 RISC-V new-PTE 修复在受控 `tg-xtask` 前置编译中带来约 18.77% 的
短窗口进度提升，同时保持 TLB/I-cache 语义回归通过。它不能单独证明完整 BuildStorm
能在 18000 秒内退出，也不能直接外推 judge 的 1616.09 秒参考值或时间分。

另一位工作人员正在使用续跑镜像做完整长测，因此本批没有并发启动第二个完整
BuildStorm，也没有修改或恢复共享镜像。长测结束后应使用本修复内核、官方 18000 秒
上限和持续资源探针完整运行一次；若超过经过 LoongArch 成功样本校准的停滞阈值，立即
停止并保存 serial、host metrics、probe latency 和 perfmap 诊断样本。
