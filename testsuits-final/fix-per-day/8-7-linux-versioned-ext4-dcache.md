# 8-7 用目录版本号让 ext4 dcache 命中时不再重复查磁盘

## 问题概述

有界 positive dcache 已经堵住了 OOM，但 ext4 每次 dcache 命中后仍然调用后端
`Inode::find()` 重新在磁盘目录块里查一遍。BuildStorm run 116 的计数器：

```text
dcache_lookups          11,516,905
dcache_backend_lookups  11,516,905   ← 100% 都去了后端
dcache_hits             10,875,096
```

命中率 94.4%，但每次命中都白白做了一次 ext4 目录块搜索。根因是 ext4 返回
`DentryCachePolicy::Revalidate`——把本地磁盘当成远程网络文件系统来对待。

直接改成 `Stable`（无条件信任缓存）又不安全：当前内核里 object-VFS 和 legacy
syscall adapter 并存，legacy 路径会绕过 dcache 直接调 ext4 的 `create`/`unlink`/
`rename`，漏掉任何一处就会让 dcache 永久返回已删除的旧文件。

解决办法：给每个目录加一个 generation（版本号），任何 namespace 变更先递增
generation 再做修改。查缓存时比对 generation——没变就信任，变了就重新验证一次。

## 背景知识

**dentry 是什么**。操作系统课讲的"目录项缓存"就是 dentry cache（dcache）。
打开 `/home/alice/code/main.c` 时，内核要逐级在磁盘上查目录：

```text
/          → 找 "home" 的 inode 号
/home      → 找 "alice" 的 inode 号
/home/alice → 找 "code" 的 inode 号
...依此类推
```

每一级都要读磁盘目录块、遍历目录项。如果把"名字→inode"的结果缓存在内存里，
下次再走同一路径就不用重复读盘了——这就是 dcache。缓存的每一条记录叫一个
dentry（directory entry，目录项缓存条目）。

**路径查找为什么开销大**。一条路径有 N 级目录就要查 N 次。每次查找要：
1. 取得目录的 inode；
2. 读目录数据块（可能不在缓存里，要走磁盘 I/O）；
3. 在数据块里线性或 hash 查找目标名字。

如果 N=5 且没有 dcache，一次 `stat()` 就要做 5 次目录搜索。BuildStorm 编译
几千个文件，每个文件的每次 open/stat/exec 都走这条路——百万次量级。

**Revalidate vs. Stable 策略**。网络文件系统（如 NFS）的服务端随时可能被别人
改动，所以客户端每次命中 dcache 都要问一句"这条还有效吗？"——这就是
Revalidate。但本地磁盘上的 ext4 只有本机内核自己会改，理论上不需要每次都去
问。Linux 的做法是：本地 ext4 根本不注册 `d_revalidate` 回调，VFS 看到没有
回调就直接信任 dcache 命中结果。

**为什么不能无条件信任（直接 Stable）**。本项目还有一条 legacy adapter 路径，
它绕过 VFS 直接调用 ext4 底层函数做 create/unlink/rename。如果 dcache 无条件
信任，legacy 路径删了文件但 dcache 不知道，之后 `stat` 会继续返回已删除文件的
信息——这是严重的正确性 bug。

**版本号（generation）怎么解决这个问题**。思路很简单：

```text
目录 /home/alice/  generation = 5

  查 "main.c" → dcache 记录：generation=5, inode=42
  再查 "main.c" → generation 仍是 5 → 直接返回 inode 42，不碰磁盘

  执行 unlink("main.c")
  → 先把 generation 递增到 6，再删除磁盘目录项

  再查 "main.c" → dcache 记录的 generation=5 ≠ 当前 6
  → 不信任缓存，去磁盘查一次
  → 磁盘说不存在 → 删掉缓存条目，返回 ENOENT
```

递增发生在修改之前，所以不会出现"已经改了但 generation 还没变"的窗口。
失败的操作可能多递增一次，只造成一次多余的重新验证，不影响正确性。

**Linux 的做法对比**。Linux 不用 generation 判断单条 dentry 是否有效，而是
要求所有目录变更都通过 VFS 的 dentry 操作（`d_instantiate`、`d_delete`、
`d_move`）来直接操作 dcache。因为 Linux 没有"legacy 绕过 VFS"的问题——所有
路径都经过 VFS。本项目的 generation 是迁移过渡方案：等 legacy adapter 全部
删除后，就可以像 Linux 一样切到 Stable + 精确失效。

Linux 也有 `i_version`（inode 版本号），但它主要给 NFS 和 `statx` 用来检测
文件内容/属性是否变过，不是用来做本地 dcache 验证的。本项目的目录 generation
更接近 Linux dcache 里的 `d_seq`（seqcount），用于检测并发修改。

## 如何发现

对 BuildStorm run 116 解析 `/proc/perf` 计数器：`dcache_backend_lookups`
等于 `dcache_lookups`，证明 100% 的缓存命中仍进入 ext4 后端。每次路径分量
平均触发 8.75 次 ext4 block-cache lookup。设备忙碌时间只占 10.8%，浪费主要
在 CPU 侧重复目录解析和 block-cache 锁竞争。

源码确认 `os/src/fs/ext4/mod.rs` 返回 `Revalidate`。

严格 A/B 对比——两个内核唯一区别是 ext4 policy：

```text
Revalidate kernel sha256:
38d2a0b8c32bee6a6d0c76cbf69e4d376781926ef0cd842a87d23ca24840f7af

Versioned kernel sha256:
437d79c88198c5fea3191517cead678939d6888c8f3c1137701c45082d065605

/user image sha256:
588f8ef482b2a9018e7c0e2dd1b0500ecdf13ac7ff955b1c401a9110ff8239af
```

LoongArch64、12 hart、8 GiB、官方只读 raw 镜像 + QEMU `-snapshot`。21 轮
`vfs_stat_smp_perf_smoke`（每轮 12 worker × 1,024 次 `newfstatat()`），首轮
warm-up 不计入中位数。

原始证据目录：

```text
testsuits-final/.tmp/final-runs/20260807-dcache-version-revalidate-debugperf-130/
testsuits-final/.tmp/final-runs/20260807-dcache-version-versioned-debugperf-131/
```

## 怎么解决

**目录版本号存储**：在稳定的 `(device_id, inode_num)` 锁表上增加
`namespace_generation: AtomicUsize`。ext4 可能为同一磁盘 inode 构造多个 Rust
对象，所以版本号绑定磁盘身份而不是对象地址。写侧先持 parent 写锁，再
Release `fetch_add` 发布新 generation，最后做磁盘修改。

**dcache 增加 Versioned 策略**：

```rust
enum DentryCachePolicy {
    Revalidate,         // 每次都问后端
    Stable,             // 无条件信任
    Versioned(usize),   // generation 没变就信任
}
```

缓存条目记录插入时的 parent generation。命中时比对：相同则直接返回；不同则
调一次后端 lookup，根据结果更新或删除缓存条目。

**覆盖全部 namespace mutation**：object-VFS 和 legacy adapter 的 create、
mkdir、symlink、link、unlink、rmdir、rename、mknod、O_CREAT、O_TMPFILE 等
全部在写锁内先调 `ext4_begin_namespace_mutation()` 递增 generation。

**正确性测试**：`path_cache_invalidation_smoke` 验证 rename/unlink 后旧名字
立即返回 ENOENT、新名字保持同一 inode。

## 对应提交

- `os/` 修复提交：`960fd0f`（`vfs: trust versioned ext4 dentries`）。
- 顶层基线：`1b919700f0eb2e49c7f0e5043549f5d5cc716cde`。
- `os/` 基线：`ff9c87df468a025dddc087bad28937032a22c80b`。

## 对比提升

21 轮 stat 微基准，丢弃首轮后 20 轮中位数：

| 指标 | Revalidate | Versioned | 改善 |
| --- | ---: | ---: | ---: |
| backend lookups | 516,852 | 30 | **-99.994%** |
| guest elapsed | 132,637 us | 114,284 us | **-13.84%** |
| host wall time | 169,594 us | 152,455 us | **-10.11%** |
| host cycles | 78.09 G | 65.70 G | **-15.86%** |
| host instructions | 220.31 G | 190.91 G | **-13.35%** |

block read 两边均为 29 次（相同），证明提升来自删除 CPU 侧重复目录搜索，不是
少读盘。剩余 ext4 cache hit 来自 stat 读 inode metadata，不是 dcache 未生效。

正确性回归 6/6 通过：`vfs_stat_smp_perf_smoke`、`path_cache_invalidation_smoke`、
`open_unlink_lifetime_smoke`、`rename_over_mmap_lifetime_smoke`、
`vfs_pathwalk_smoke`、`unix_vfs_path_smoke`。

本批没有重跑完整 BuildStorm（约 1 小时）。下次 BuildStorm 应确认
`dcache_backend_lookups / dcache_lookups` 比例降到接近 cold lookup + mutation
后首次 lookup 的规模。

以下是 AI 的具体分析，作为存档。

---

## 历史分析背景

这个优化是 dcache 系列的第三步：先堵 OOM（有界 dcache + slab），再限制回收延迟
（CLOCK 预算），最后让命中真正跳过后端（本文的 generation）。问题本身不难理解，
但修改需要覆盖 object-VFS 和 legacy adapter 的所有 namespace mutation 路径，
漏掉任何一处都会导致 stale positive——所以保留了完整的 mutation 覆盖清单和
Linux 对照分析。


## 问题概述

BuildStorm run 116 已经证明 bounded strong-reference dcache 解决了 OOM，但它没有
减少 ext4 目录查找：

```text
dcache_lookups          11,516,905
dcache_backend_lookups  11,516,905
dcache_hits             10,875,096
```

旧实现虽然能在 map 中找到 94.4% 的 positive dentry，ext4 却返回
`DentryCachePolicy::Revalidate`，所以每次命中仍调用 `Inode::find()`。完整运行中因此
产生 100,738,945 次 ext4 block-cache hit；本地、不可被外部服务器修改的 ext4 被当成
网络文件系统使用，dcache 只保留对象身份，不承担路径查找缓存的职责。

直接把 ext4 改成 `Stable` 仍不安全。当前 object-VFS 和 legacy syscall adapter 并存，
后者仍直接调用 `ext4_fs::Inode::{create_*,link_inode,unlink,rename}`。只要漏掉一个显式
dentry invalidation，unlink 或 rename 后就可能永久返回旧 positive。

本批新增每目录 namespace generation：generation 未变化时 positive dentry 是可信热
缓存；所有目录变更在实际修改前发布新 generation。这样既去掉重复 backend lookup，
又给迁移期的 legacy 路径提供统一失效边界。

## 如何发现

### perf、计数器与运行日志

专家对成功的完整 BuildStorm run 116 解析 `/proc/perf` 后发现：

- `dcache_backend_lookups == dcache_lookups`，证明 ext4 热命中仍 100% 进入后端；
- 每次路径分量平均触发 8.75 次 ext4 block-cache lookup；
- block device 忙碌时间只占总运行时间约 10.8%，主要浪费在 CPU 侧的重复目录解析和
  全局 block-cache 锁竞争，而不是设备吞吐。

源码核对确认 `os/src/fs/ext4/mod.rs` 返回 `Revalidate`；仓库中的其他动态文件系统也
采用该策略，`Stable` 快路径在真实 ext4 workload 中没有使用者。

本批用同一当前源码构建严格 A/B，两颗诊断内核的唯一行为差异是 ext4 policy：

```text
Revalidate kernel sha256:
38d2a0b8c32bee6a6d0c76cbf69e4d376781926ef0cd842a87d23ca24840f7af

Versioned kernel sha256:
437d79c88198c5fea3191517cead678939d6888c8f3c1137701c45082d065605

/user image sha256:
588f8ef482b2a9018e7c0e2dd1b0500ecdf13ac7ff955b1c401a9110ff8239af
```

两边都临时设置 `DEBUG_PERF=true`，使用 LoongArch64、12 hart、8 GiB、官方只读 raw
镜像加 QEMU `-snapshot`。启动后附加 host `perf stat`，连续运行 21 轮
`vfs_stat_smp_perf_smoke`；每轮 12 个 worker 各做 1,024 次 `newfstatat()`，共
12,288 次 stat，全部 `errors=0`。首轮作为 warm-up，不进入中位数。

原始证据：

```text
testsuits-final/.tmp/final-runs/20260807-dcache-version-revalidate-debugperf-130/
testsuits-final/.tmp/final-runs/20260807-dcache-version-versioned-debugperf-131/
```

每个目录都包含 `serial.log`、`results.csv`、`perf-stat.csv` 和
`host-metrics.log`。本轮没有给计时 QEMU 加 `-perfmap`：热点已经由 run 116 和直接
计数器锁定，`-perfmap` 会改变 TCG 翻译条件；需要再次归因 guest PC 时仍应按
`testsuits-final/AGENTS.md` 使用短时 `perf record + -perfmap` 并保存准确 PID 的 map。

### Linux 对照

对照本地 Linux 7.1-rc7，commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`：

- `fs/namei.c:1026-1030 d_revalidate()` 只有在 dentry 设置
  `DCACHE_OP_REVALIDATE` 时才调用文件系统回调；否则直接返回有效；
- `fs/dcache.c:1971-1972` 只有文件系统提供 `d_revalidate` op 才设置该 flag；
- `fs/ext4/` 中没有 `d_revalidate` 实现，所以本地 ext4 positive dentry 命中不会重复
  执行 `ext4_lookup()`；
- `fs/namei.c:1838-1863 lookup_fast()` 先走 `__d_lookup_rcu()`，并用 dentry
  seqcount 校验并发变化；
- `include/linux/dcache.h:96` 的 `d_seq`、`fs/dcache.c:3041 d_move()` 和
  `fs/dcache.c:2584 d_delete()` 让 rename/unlink 与 lockless path walk 协调；
- ext4 在目录项变化处调用 `inode_inc_iversion(dir)`，
  `include/linux/iversion.h:47` 也明确把 mkdir/rmdir/unlink 等 namespace change 计入
  directory i_version。

Linux 并不把 `i_version` 当成本地 ext4 每个 positive dentry 的常规 tag；它依赖所有
namespace mutation 都经过 VFS dentry 生命周期操作。本内核尚未完成 legacy adapter
迁移，因此 generation 是借鉴 `i_version + seqcount` 的安全过渡，不声称是一比一移植。

## 怎么解决

### 1. 在稳定 ext4 inode 身份上保存目录版本

`Ext4InodeLock` 从单纯的 `KernelRwSemaphore<()>` 扩展为：

```rust
pub(crate) struct Ext4InodeLock {
    semaphore: KernelRwSemaphore<()>,
    namespace_generation: AtomicUsize,
}
```

ext4-fs 可能为同一磁盘 inode 构造多个 Rust `Inode` wrapper，所以状态仍按
`(device_id, inode_num)` 放在既有 weak table 中，而不是绑定对象地址。这对应 Linux
把 `i_rwsem` 和 inode change state 放在唯一的 `struct inode` 上。

写侧必须先持 parent write semaphore，再以 Release `fetch_add` 发布 generation，最后
执行第一次磁盘目录修改。读侧 policy 以 Acquire load 取得 generation。

### 2. dcache 增加 Versioned policy

新增 `DentryCachePolicy::Versioned(usize)`，`CachedDentry` 记录插入或验证时的 parent
generation：

- generation 相同：直接返回缓存 dentry，不调用 backend；
- generation 不同：调用一次 backend lookup；
- 后端仍返回同一 inode：保留已发布的 dentry identity，只更新 generation；
- 后端返回不同 inode：替换该 stable key 的 positive；
- 后端返回 `ENOENT`：删除旧 positive。

版本检查没有改变既有 32K 有界 CLOCK、强引用生命周期和 parent identity 校验。

### 3. 覆盖全部 ext4 namespace mutation

object-VFS 的 create、mkdir、symlink、link、unlink、rename 和 mknod 全部在 parent
写锁内发布 generation。legacy adapter 的以下路径也统一调用
`ext4_begin_namespace_mutation()`：

- open/O_CREAT 与 O_TMPFILE pool 创建；
- mkdir、mknod、symlink、link、unlink/rmdir；
- 同目录 rename、跨目录 rename、rename exchange 及其 rollback；
- open-unlinked 文件的 hidden-name rename 和最终清理；
- root mount directory 的延迟创建。

发布发生在实际 mutation 之前。若线程在取得写锁后、发布前暂停，旧 fast lookup 可以
线性化在 mutation 之前；发布之后的新 lookup 会 miss，并在 backend `lookup()` 的
read semaphore 上等待写侧完成。因此不会观察“新 generation + 半完成目录修改”。失败
操作可能多推进一次 generation，只造成一次保守 revalidation，不影响语义。

### 4. 增加 mutation 回归

VFS 单测新增 versioned cache 用例，覆盖“热命中不访问 backend、generation 改变后重新
查找并替换旧 inode、cache key 数量不增长”。由于 os crate 的 native test target 仍受
既有架构耦合影响，该单测目前不能直接作为 host `cargo test` 执行结果。

为运行真实生产路径，重新接入已有 `path_cache_invalidation_smoke`。它先重复 stat 建立
热 positive，再执行 rename/unlink，验证旧名字立即 `ENOENT`、新名字保持同一 inode，
最后再次 unlink 并验证不存在。

### 更完整的后续方案

当前 generation 会在同一目录任意名字变化后保守地使该目录所有 cached positive
重新验证一次。最终应继续向 Linux 靠拢：

1. 所有 ext4 namespace mutation 都通过 object-VFS，删除 legacy 直调后把 ext4 改为
   `Stable`，只对受影响的 dentry 执行 targeted instantiate/delete/move；
2. 用分桶 hash + RCU/seqcount 替换单个 `RwLock<BTreeMap>`，去掉多 hart 读侧锁和
   O(log n) 查找；
3. 增加 negative dentry，并让 positive/negative dentry、inode 和 page cache 进入统一
   shrinker/内存水位体系；
4. 保留 generation 作为 debug consistency check，检测未来新增 mutation 是否漏失效，
   而不是永久作为全目录失效机制。

## 对因提升

### 直接计数器

21 轮相同 workload 的 `/proc/perf` 前后差值：

| 指标 | Revalidate | Versioned | 改善 |
| --- | ---: | ---: | ---: |
| dcache lookups | 516,852 | 516,852 | workload 相同 |
| backend lookups | 516,852 | 30 | **-99.994%** |
| revalidated hits | 516,823 | 1 | **-99.9998%** |
| ext4 cache hits | 1,812,838 | 1,295,954 | -28.51% |
| block reads | 29 | 29 | 相同 |

block read 完全相同，说明这个已预热微基准的提升来自删除 CPU 侧重复目录解析和锁竞争，
不是某轮碰巧少读盘。剩余 ext4 cache hit 主要来自 stat 读取 inode metadata，本批没有
把它误算成 dcache 未生效。

### guest 与 host 耗时

丢弃首轮后 20 轮中位数：

| 指标 | Revalidate | Versioned | 改善 |
| --- | ---: | ---: | ---: |
| guest elapsed | 132,637 us | 114,284 us | **-13.84%** |
| host wall time | 169,594 us | 152,455 us | **-10.11%** |

### host perf stat

`perf stat` 在 guest 启动后附加到准确 QEMU PID，并覆盖全部 21 轮；task-clock 是 12 个
QEMU vCPU 线程的累计时间，不是墙钟：

| perf 指标 | Revalidate | Versioned | 改善 |
| --- | ---: | ---: | ---: |
| task-clock | 32,777.79 ms | 27,446.33 ms | **-16.27%** |
| cycles | 78,086,798,941 | 65,698,380,072 | **-15.86%** |
| instructions | 220,313,825,612 | 190,907,627,920 | **-13.35%** |
| branches | 31,521,203,428 | 27,469,345,716 | **-12.85%** |
| branch misses | 59,124,202 | 45,994,526 | **-22.21%** |

guest 时间、直接 backend 计数和 host perf 指令数同向改善，证据链证明删除的正是重复
文件系统工作，而不是放宽正确性或跳过测试。

## 正确性与构建验证

恢复 `DEBUG_PERF=false` 后构建正式 LoongArch kernel，sha256：

```text
e5389341524b98be335261ac12c0a9f8a163cb371a1853ee6730c781948b49ba
```

正式 QEMU 回归目录：

```text
testsuits-final/.tmp/final-runs/20260807-versioned-dcache-final-regressions-132/
```

每项 guest 60 秒、外部 70 秒硬截止，6/6 通过：

| 测试 | host 时间 | 结果 |
| --- | ---: | --- |
| vfs_stat_smp_perf_smoke | 199 ms | 12,288 stat，errors=0 |
| path_cache_invalidation_smoke | 44 ms | rename/unlink 后无 stale positive |
| open_unlink_lifetime_smoke | 163 ms | PASS |
| rename_over_mmap_lifetime_smoke | 73 ms | 32/32 PASS |
| vfs_pathwalk_smoke | 46 ms | PASS |
| unix_vfs_path_smoke | 44 ms | PASS |

其他验证：

- exact-source dcache host harness：7 passed、0 failed、1 个手工性能基准 ignored；
- `cargo test -p ext4-fs --target x86_64-unknown-linux-gnu`：13 passed；
- RISC-V `riscv64gc-unknown-none-elf` 与 LoongArch
  `loongarch64-unknown-none-softfloat` `cargo check` 均通过；
- touched Rust files 的 `rustfmt --check` 与 `git -C os diff --check` 通过；
- 最终 `DEBUG_PERF=false`，测试结束后无 QEMU/perf 进程残留。

本批聚焦证明 pathname/stat 热路径改善和 namespace mutation 语义，没有声称已重新完成
一轮约一小时的 BuildStorm。下一次完整 BuildStorm 应继续记录
`dcache_backend_lookups / dcache_lookups`，确认真实构建中后端比例也从 100% 降到接近
cold lookup 与 mutation 后首次 lookup 的规模。

## 对应提交

- 顶层基线：`1b919700f0eb2e49c7f0e5043549f5d5cc716cde`；
- `os/` 基线：`ff9c87df468a025dddc087bad28937032a22c80b`；
- `os/` 修复提交：`960fd0f`（`vfs: trust versioned ext4 dentries`）；
- 顶层提交包含子模块指针、测试注册和本报告。

## AI 使用说明

专家提供 run 116 的计数器审查和 Linux 对照方向；Codex 重新核对当前源码与本地 Linux，
设计目录版本并发协议，审计全部 namespace mutation，实施修改，构建同源 A/B，运行
host perf、guest 计数器和受硬截止保护的 QEMU 回归。表格均来自上述原始 CSV/日志。
