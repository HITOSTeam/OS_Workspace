# 8-4 BuildStorm PreparedWait 退出与性能实验复核

## 1. 结论

本批次没有运行完整 BuildStorm。工作集中在前一轮 `tg-xtask` 卡顿现场暴露出的
多线程退出边界，并用聚焦运行态回归验证修复：

- `PreparedWait` 现在会在“已收到唤醒”“提交调度前”和“被调度回来后”三个边界
  检查 exec de-thread 请求与默认致命信号；
- 阻塞在 `epoll_wait()` 的同组线程会在 `exit_group()` 或另一个线程 `execve()` 时
  完成退出，不再把致命 teardown wake 当作普通事件后重新睡眠；
- 新增双场景用户态回归，同时覆盖普通进程组退出和 exec de-thread；
- RISC-V64、LoongArch64 内核和新增用户回归的静态构建均通过；RISC-V 8 hart
  运行态回归输出两项 PASS，返回码为 0。

本批次还复核了两个看似 Linux 化、但证据不足或实际退化的性能方案：

- 64 桶 futex wait queue 没有在受控窗口内证明进度收益，已完全回退；
- “分片 buddy + per-hart 小对象缓存”虽然类似 Linux SLUB/PCP，但三轮 IOZone
  中位总耗时比原分片 buddy 慢 3.9%，并且写吞吐下降，已完全回退。

最终保留的内核 diff 只有 PreparedWait 致命退出修复。不能据此声称完整 BuildStorm
已经通过，也不能把被回退实验算作性能提升。

## 2. 版本与测试资产

| 资产 | 值 |
| --- | --- |
| 顶层基线 | `1c8f6670bf6767750bd893772f6c0eacc1a1e56a` |
| `os/` 基线 | `04bf218113d8f4045db9750f751ab5fd58fbcc22` |
| `os/` 最终 revision | `e625f82f3da80b315baf6cbe8627245e83bb218d` |
| 顶层回归提交 | `190573fd5c08405fffad55288c11ad3e1eb3c38d` |
| 顶层内核集成提交 | `927328f62ecfb753c9c4cb0bd825088be304f48e` |
| final 测试分支/commit | `final-2026` / `1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36` |
| 本地 Linux 参考树 | `fc02acf6ac0ccde0c805c2daa9148683cdd01ba8` |
| QEMU | 11.0.3 |
| 最终运行态 guest | RISC-V64，8 vCPU，8 GiB，snapshot |
| 决赛根镜像 | `sdcard-rv-pub.img`，14 GiB raw ext4 |
| 决赛镜像 SHA-256 | `d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1` |
| IOZone 临时镜像 SHA-256 | `c0cb4e209e0aa243af72c599c3e34a679ff1f195ac46789e054b77eac3bf453d` |

`run.sh` 在启动最终回归前重新扫描并确认了 14 GiB 决赛镜像校验值。运行使用 QEMU
snapshot，没有修改基准镜像。final 测试源码未拉取或更新。

## 3. 失败根因

前一轮为避免条件检查与真正睡眠之间丢唤醒，引入了 `PreparedWait`：waiter 在条件锁
保护下把任务发布为 `Blocked`，然后携带 irq-save guard 完成最终检查和 scheduler
commit。这个协议封住了普通事件的 lost wakeup，但它绕过了旧的
`block_current_and_run_next_impl(true)`，后者原本会在睡眠前检查：

1. exec de-thread 的 task-local exit token；
2. 默认动作是终止进程的 pending signal。

当 `exit_group()` 或 `execve()` 的 de-thread 协调者唤醒一个 epoll waiter 时，旧
`PreparedWait::sleep()` 只消费 `wakeup_pending` 并返回。对普通 I/O 事件这是正确的，
但对 teardown wake 不够：尤其 exec token 不是一个普通 SIGKILL 位，epoll readiness
循环无法通过自己的 signal scan 识别它，于是可能再次注册 waiter 并睡眠，使 exec
协调者一直等不到 peer retirement。

退出清理自身还可能在旧 mm 的冷页访问或块 I/O 上让出 CPU。此时
`TaskUserRes` 已被取走，不能再次递归进入一次性线程退出流程，否则会重复消费
`LiveThreadRetirement` 等生命周期票据。

## 4. Linux 对照

本次直接阅读本地 `exampleOs/linux`，没有逐行移植：

| Linux 机制 | 参考文件 | 本项目对应实现 |
| --- | --- | --- |
| 入队后发布 sleep state，封闭 lost wakeup | `kernel/sched/wait.c:prepare_to_wait()` | `PreparedWait::new()` + `wakeup_pending` |
| 在 wait-queue 锁下拒绝带 pending signal 的 interruptible/killable sleep | `kernel/sched/wait.c:prepare_to_wait_event()`、`include/linux/sched/signal.h:signal_pending_state()` | PreparedWait 三个 fatal-teardown 检查点 |
| `exit_group()` 发布 group exit 并杀死其他线程 | `kernel/exit.c:do_group_exit()`、`kernel/signal.c:zap_other_threads()` | group SIGKILL pending bit + wake |
| exec 杀死 peer，并以 killable wait 等待线程数归零 | `fs/exec.c:de_thread()` | task-local exec exit token + peer retirement counter |

Linux 的关键语义不是“任何 wake 都可以继续等待”，而是 interruptible/killable waiter
在提交 sleep 时必须同时观察 fatal signal。Linux 的 exec peer 使用 SIGKILL；本项目
为了不把 peer teardown 重新广播给 exec owner，使用 task-local token，因此必须在
相同的 prepared-sleep 边界显式检查 token。

## 5. 实现

`os/src/task/processor.rs` 新增
`PreparedWait::exit_for_fatal_teardown_if_requested()`，检查顺序为：

1. 若 `TaskUserRes` 已被取走，说明当前栈正在执行退出清理，继续原清理流程；
2. 若 task-local exec exit token 已发布，解除 token 自身的 armed/irq 状态并进入
   `exit_current_and_run_next(0)`；
3. 若存在默认致命 signal，记录退出原因并进入 `exit_group_and_run_next()`。

调用点覆盖三种竞态：

```text
prepare Blocked
    -> 已有 wake_pending：恢复 Running 后检查 teardown
    -> 尚无 wake：提交 schedule 前检查 teardown
    -> schedule 被远端 wake 后返回：再次检查 teardown
```

普通 readiness wake 仍按原协议返回调用者重查条件；没有引入轮询、固定延时或测试名
特判。

新增 `user/src/bin/smoke_archive/exec_epoll_thread_smoke.rs`：

- 子进程创建一个同 mm/sighand/thread-group 的线程，让其无限阻塞在 epoll；主线程
  调用 `exit_group(0)`，父进程用 `waitpid()` 验证整个组可退出；
- 当前进程再次创建同类 epoll waiter，主线程 exec `/user/basename.bin`；新映像输出
  `PASS`，证明 exec peer 已退休且 exec 没有被错误的 group SIGKILL 杀死。

## 6. 运行态验证

最终命令：

```sh
ARCH=riscv64 SMP=8 MEM=8G ./run.sh shell
/user/exec_epoll_thread_smoke.bin; echo PREPARED_WAIT_RC=$?
```

串口日志：

```text
.tmp/final-runs/20260804-013432-riscv64-shell/serial.log
```

关键输出：

```text
EXIT_GROUP_PREPARED_WAIT_PASS
PASS
PREPARED_WAIT_RC=0
```

启动日志同时确认 `riscv64 SMP online mask 0xff (8 harts), failed=0x0`。

## 7. 分配器 IOZone A/B 与回退决定

实验配置：RISC-V64、8 vCPU、2 GiB、QEMU snapshot，4 个 IOZone worker，每个
4 MiB 文件，1 KiB record，只运行 sequential write/rewrite/read/reread。命令：

```sh
IOZONE_RUNS=3 IOZONE_WORKLOAD=sequential \
  bash tools/run_iozone_focus.sh \
  /Users/bytedance/projects/OS_Workspace \
  .tmp/iozone/iozone-root.img \
  <log>
```

三轮中位数如下，吞吐单位为 KiB/s：

| 实现 | 总耗时/s | initial write | rewrite | read | reread |
| --- | ---: | ---: | ---: | ---: | ---: |
| 原 per-hart 分片 buddy | 19.15 | 5154.16 | 17082.61 | 14715.09 | 14573.42 |
| 分片 buddy + 小对象缓存 | 19.90 | 4737.26 | 14800.60 | 14468.31 | 13616.02 |
| 缓存方案相对变化 | **慢 3.9%** | -8.1% | -13.4% | -1.7% | -6.6% |

原始日志：

```text
.tmp/iozone/8-4-heap-sharded-3run.log
.tmp/iozone/8-4-heap-hybrid-3run.log
```

Linux SLUB/PCP 的 CPU-local freelist 不仅是一组本地链表，还依赖成熟的 slab/page
分层、批量 refill/drain、水位控制和 NUMA/zone 策略。把小对象缓存直接叠在当前 buddy
分片上增加了 refill/drain 与碎片化成本，本场景没有收益。因此实验代码已完全回退，
不把“形式上更像 Linux”当成交付理由。

## 8. 静态验证

以下检查均返回 0：

```sh
ARCH=riscv64 cargo check --offline --manifest-path os/Cargo.toml \
  --target riscv64gc-unknown-none-elf

ARCH=loongarch64 cargo check --offline --manifest-path os/Cargo.toml \
  --target loongarch64-unknown-none-softfloat

cargo check --offline --manifest-path user/Cargo.toml \
  --bin exec_epoll_thread_smoke --target riscv64gc-unknown-none-elf

cargo check --offline --manifest-path user/Cargo.toml \
  --bin exec_epoll_thread_smoke --target loongarch64-unknown-none

git -C os diff --check
git diff --check
```

构建仅有仓库既有 warning，没有新增编译错误。`user/.cargo/config.toml` 在测试结束后
保持 RISC-V 默认 target。没有运行完整 BuildStorm、完整 LTP、CAgent 或完整性能套件。

## 9. 适用边界与下一步

- 当前 PreparedWait 仍是项目的锁保护 REF-wait 模型，没有实现 Linux waitqueue 的
  exclusive waiter、restart block 或完整 task-state 位图；
- 运行态聚焦回归已覆盖 RISC-V；LoongArch 本批次完成静态检查，先前同一修复的
  LoongArch 聚焦回归也通过，但不把旧日志冒充本次最终运行；
- 下一步应重新运行受控的 `tg-xtask` 聚焦窗口，确认 de-thread 卡点消失，再依据新的
  PC/进度证据选择下一个瓶颈；在它能稳定完成以前仍不应运行或宣称完整 BuildStorm。
