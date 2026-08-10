# 热路径诊断计数去共享写竞争

## 问题概述

冷启动 `tg-xtask` 会编译约 354 个单元，产生大量 syscall 与 ext4 block-cache
访问。生产内核原先在两条高频路径上维护全局原子状态：

- 每次 syscall 都向 7 个全局 `AtomicUsize` 写入 syscall 号和 6 个参数，仅供 OOM
  现场打印；
- 每次 block-cache 命中、缺失、等待和淘汰都更新全局原子计数，即使
  `DEBUG_PERF=false` 也不会被编译器删除。

这些计数不改变系统语义，却会让 RV8/LA12 的 hart 反复争用同一 cacheline。
历史 BuildStorm 诊断轮曾记录超过一亿次 block-cache hit，因此首先把它作为
低风险候选处理。对应提交为：

- `8b8fe01 perf: localize hot diagnostic counters`（内核 syscall 诊断）；
- `eb86586e perf: compile out block cache statistics`（ext4 与特性接线）。

## 如何发现

### 运行证据

旧 run116 的 `/proc/perf` 记录过 `ext4_cache_hits=100,738,945`。即使 Relaxed
原子不提供同步语义，它仍然是多个 hart 对同一 cacheline 的 read-modify-write。
源码审计又确认 syscall 入口无条件执行 7 次全局 store，而 trap handler 已经在
当前 task 中保存了可重启 syscall 状态，二者功能重复。

本批使用精确的冷 `tg-xtask` runner：每轮从不可变 raw backing 创建 fresh qcow2
overlay，删除 `target/debug`，记录 `TGXTASK_BEGIN/TGXTASK_END`、依赖数、串口字节、
QEMU RSS/CPU/I/O 和探针延迟。耗时门禁关闭 `-perfmap` 与 `perf record`，避免诊断
机制本身扰动结果。

### Linux 对照

本地 `exampleOs/linux` 的块层统计不会让所有 CPU 写同一个原子：

- `include/linux/part_stat.h:57-69` 的 `part_stat_add/inc()` 使用
  `__this_cpu_add()`；
- `include/linux/vmstat.h:68-78` 的 VM event 也使用 `this_cpu_inc/add()`；
- 只用于调试的统计通常受 `CONFIG_*`、静态分支或 debugfs 配置控制，例如
  `include/linux/vmstat.h:117-125` 与 `kernel/smp.c:140-199`。

因此本批采用相同原则：生产热路径不为可选诊断付共享写成本；需要诊断时由
显式构建特性恢复计数，而非永久启用。

## 怎么解决

### syscall OOM 现场改为 per-hart

`LAST_SYSCALL_*` 改为 128 字节对齐的 `LastSyscallSlot[MAX_HARTS]`。syscall 入口
只写当前 hart 的槽，避免 hart 间 cacheline bouncing。OOM 处理仍能在不获取 task
锁、不触发新分配的条件下读取当前 hart 的最后一次 syscall。

这里没有在 OOM 路径直接锁 task：分配失败可能发生在持锁或锁实现内部，诊断
代码再次获取 task 锁会把可打印的 OOM 变成死锁。

### block-cache 统计在生产构建中完全删除

`ext4-fs` 新增 `block-cache-stats` feature：

- ext4-fs 自身默认开启，保留单测和独立诊断行为；
- `os` 依赖使用 `default-features=false`，生产内核不生成统计原子与热路径指令；
- `os --features perf_diagnostics` 显式恢复全部 block-cache 计数，供 `/proc/perf`
  诊断使用；
- feature 关闭时 `cache_diagnostics()` 仍返回容量，其他可选字段为 0，接口不漂移。

第一版曾把 hit/miss 放进 `BLOCK_CACHE_MANAGER` 已持有的全局锁内，试图避免额外
原子。LA12 冷启动 300 秒门禁中，候选两轮只有 125 个依赖，基线为 126/127。
这是把统计工作加进唯一 manager 临界区造成的串行化，已经否决。最终版本使用
编译期删除，不延长该锁。

新增 `lookup_counters_count_each_request_once`，验证诊断构建中一次 cold lookup
只记一次 miss、后续 warm lookup 只记一次 hit。

## 对应提升

### 300 秒 B-C-C-B 拒绝型门禁

环境统一为 QEMU 11.0.3、8 GiB、`DEBUG_PERF=false`、perfmap off、30 秒探针；
LoongArch 使用 12 hart，RISC-V 使用 8 hart 与 `rv64,svvptc=true`。

| 架构 | B1 | C1 | C2 | B2 | 候选相对结果 |
| --- | ---: | ---: | ---: | ---: | --- |
| LoongArch | 127 | 126 | 127 | 127 | 中位数约 -0.4%，中性 |
| RISC-V | 90 | 91 | 90 | 89 | 中位数约 +1.1%，小幅正向 |

对应运行目录：

- LA：`649`、`650`、`651`、`652`；
- RV：`654`、`655`、`656`、`657`。

LA 的 run648 在 rustc 创建 coordinator thread 时收到一次 `EAGAIN`，`rc=101`；
相同基线 run649 正常到 127 个依赖，因此 648 被标为无效样本，没有混入 A/B。
RV 的 run653 因 runner 误挂 LoongArch `user.ext4`，`init_proc` 返回 `ENOEXEC`，也
被标为夹具错误并重跑。

### 完整冷 `tg-xtask`

| 架构 | 运行 | 精确起止 guest uptime | 冷编译耗时 | 结果 |
| --- | --- | --- | ---: | --- |
| LoongArch | run658 | 0.71 → 2326.45 s | 2325.74 s | rc=0 |
| RISC-V | run659 | 0.73 → 2654.26 s | 2653.53 s | rc=0 |

LoongArch 旧 run121 只能把完成点夹在 `(2246.63, 2306.88]s`，没有本批的精确
start/end 标记；候选比该窗口上界慢约 0.8%，不足以证明端到端提升。RISC-V 也
没有同版本的完整冷基线。因此严格结论是：**本批消除了不必要的共享写，RISC-V
短门禁约有 1% 信号，LoongArch 与完整耗时均为中性，未证明分钟级收益。**

这符合预期：它是低风险清理，不是 38 分钟长耗时的主解。下一主线应继续统一
inode `address_space`，让 read/pread/mmap/exec 共用文件页，并消除 per-open
128 KiB 数据缓冲。

## 验证

```zsh
cargo test --manifest-path ext4-fs/Cargo.toml \
  --target x86_64-unknown-linux-gnu
cargo check --manifest-path ext4-fs/Cargo.toml \
  --no-default-features --target x86_64-unknown-linux-gnu

TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --manifest-path os/Cargo.toml \
  --target riscv64gc-unknown-none-elf
TMPDIR=$PWD/.tmp ARCH=riscv64 cargo check --manifest-path os/Cargo.toml \
  --target riscv64gc-unknown-none-elf --features perf_diagnostics
TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check --manifest-path os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat
TMPDIR=$PWD/.tmp ARCH=loongarch64 cargo check --manifest-path os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat --features perf_diagnostics
```

- ext4-fs：16 passed / 0 failed；
- 默认、无统计、显式诊断三种构建路径均通过；
- 两架构 release ELF 经 `llvm-nm -C` 确认不含
  `ext4_fs::block_cache::diagnostics` 统计符号；
- 决赛 suite commit：`b5ec6ef8497e1818cbdec3b54bb722f036e57972`；
- RV root SHA-256：`d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1`；
- LA root SHA-256：`2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a`；
- 候选 ELF SHA-256：RV
  `49328b3497e15d87500f15d5c490a4dd413784cb2b20f604d953ce2be7d802a4`，LA
  `cf6aab0b4c8704cc5622369cf7e789a5e25bbf08b6b6d84cedffbf131158f2dc`。

## AI 使用说明

AI 用于审计热路径、对照本地 Linux 7.1-rc7 源码、实现 feature/per-hart 方案、
编排 fresh-overlay B-C-C-B 与完整冷启动运行，并交叉检查串口、guest 探针和宿主
资源。所有性能结论均来自上述可复现日志；无伪造计时或输出。
