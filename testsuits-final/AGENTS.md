# 决赛测试工作区说明

## 适用范围

本目录存放 2026 年全国大学生 OS 比赛内核赛道决赛测试资产，用于决赛测例的
接入、诊断和性能优化。

## 总体目标与 Linux 标准

总体目标是在行为正确的前提下，尽可能快地完成全部 final 测试。实现和评审时
以 Linux 的用户可观察语义、ABI 和并发正确性作为标准，同时把吞吐、延迟、
内存占用和多核扩展性作为核心约束。

不要求逐行移植 Linux，也不要求完整复制与当前决赛场景无关的内部结构、边界
功能和实现细节。允许采用更简单、更适合本项目架构的机制，但必须满足：

- final 测试依赖的 Linux 行为正确，错误码、同步、资源生命周期和并发结果可信；
- 机制可复用、可维护，不针对单个测试名称、固定输入或评分脚本硬编码；
- 不以牺牲初赛/LTP 已支持语义、稳定性或安全性换取局部速度；
- 优先修复真实瓶颈，避免为了形式上接近 Linux 引入不必要的复杂度和开销；
- 对尚未实现的 Linux 细节明确记录适用边界，并确认不会影响当前 final 测试。

判断一项实现是否可接受时，顺序是：正确性与 ABI、并发和资源安全、final
测试完成度、性能、内部实现相似度。Linux 源码用于确认语义和成熟设计，不是
要求内部实现一比一复制。

优先在 `../os/` 中实现符合 Linux 语义、可复用且可维护的子系统修复。禁止为
单个测例硬编码返回值、伪造输出或破坏初赛测例兼容性。

## 开始前必读

开始决赛工作前，先检查并验证以下资产：

- `testsuits-for-oskernel/` 存在，当前分支为 `final-2026`，并记录当前 commit；
- 当前目标架构对应的 `sdcard-rv-pub.img` 或 `sdcard-la-pub.img` 存在；
- 镜像是 ext4 文件系统，且 SHA-256 与“本地资产”中记录的值一致。

如果源码缺失，从
`https://github.com/oscomp/testsuits-for-oskernel` 克隆 `final-2026`
分支；如果因网络或权限问题无法克隆，应向用户报告。镜像体积较大且由比赛方
单独分发，如果缺失，应提示用户按照决赛源码 `README.md` 中的官方地址下载，
不得使用来源不明的镜像代替。

修改内核或测试接线前，先阅读：

- `testsuits-for-oskernel/README.md`
- `testsuits-for-oskernel/scripts/cagent_testcode.sh`
- `testsuits-for-oskernel/scripts/buildstorm_testcode.sh`
- `testsuits-for-oskernel/judge/judge_cagent-glibc.py`
- `testsuits-for-oskernel/judge/judge_buildstorm-glibc.py`

决赛测例源码应保持在 `final-2026` 分支。评分常量和脚本可能更新，因此每份
测试报告都必须记录实际使用的源码 commit。

## 本地资产

比赛方后续可能更新测试镜像、脚本或评分规则。开始新的决赛开发批次时，应：

1. 只读检查远端 `final-2026` 分支 HEAD 和官方 README/公告；
2. 将远端 commit 与本地记录对比；
3. 发现更新时记录新旧 commit、镜像版本或 checksum 的差异并通知用户；
4. 未经用户明确确认，不得自动拉取源码、替换基准镜像或修改已有测试基线。

- `sdcards-final2.1.tar.xz`：下载的镜像压缩包。
- `sdcard-rv-pub.img`：RISC-V64 决赛根文件系统。
- `sdcard-la-pub.img`：LoongArch64 决赛根文件系统。
- `testsuits-for-oskernel/`：从
  `https://github.com/oscomp/testsuits-for-oskernel` 浅克隆的
  `final-2026` 分支。

2026-07-29 检查到的镜像信息：

| 镜像 | SHA-256 | Rust 工具链 |
| --- | --- | --- |
| `sdcard-rv-pub.img` | `d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1` | `nightly-2026-05-28-riscv64gc-unknown-linux-gnu` |
| `sdcard-la-pub.img` | `2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a` | `nightly-2026-05-28-loongarch64-unknown-linux-gnu` |

两个镜像均为：

- 14 GiB raw 镜像；
- 干净的 ext4 文件系统，卷标为 `starry-rootfs`；
- `/root/.cargo` 中带有离线 Cargo 缓存；
- `/work/tgoskits` 中带有 BuildStorm 源码工作区；
- `/glibc` 中带有决赛可执行文件和测试脚本。

上级目录中的 `../testsuits-for-oskernel/` 是初赛测例快照，不要与本目录的
`final-2026` 决赛源码混用。

## 保护基准镜像

将两个 `sdcard-*-pub.img` 及其压缩包视为不可变的基准资产。BuildStorm 会向
`/tmp`、`/work` 和 Cargo target 目录写入大量数据，因此禁止直接把唯一的基准
镜像作为可写 raw 磁盘启动。应使用一次性副本，或基于 raw 镜像创建 QEMU
qcow2 overlay。

禁止：

- 使用 `debugfs -w` 原地修改基准镜像；
- 在宿主机上以读写方式挂载基准镜像；
- 将镜像、压缩包、完整根文件系统或嵌套源码仓库加入上级仓库的提交；
- 在镜像校验值和测试流程尚未备份前删除压缩包。

在 Homebrew/macOS 上只读检查：

```zsh
E2FS_DEBUGFS="$(brew --prefix e2fsprogs)/sbin/debugfs"
E2FS_FSCK="$(brew --prefix e2fsprogs)/sbin/e2fsck"
"$E2FS_DEBUGFS" -R 'ls -l /' sdcard-la-pub.img
"$E2FS_DEBUGFS" -R 'ls -l /glibc' sdcard-la-pub.img
"$E2FS_DEBUGFS" -R 'ls -l /work/tgoskits' sdcard-la-pub.img
"$E2FS_FSCK" -fn sdcard-la-pub.img
shasum -a 256 sdcard-la-pub.img
```

检查 RISC-V 镜像时，将命令中的文件名替换为 `sdcard-rv-pub.img`。

## 决赛运行脚本

使用本目录的 `run.sh` 构建内核、挂载对应架构的决赛镜像并启动 QEMU：

```zsh
# 进入交互式 CongCore shell
ARCH=riscv64 ./run.sh shell

# 自动运行 CAgent 并调用本地 judge
ARCH=loongarch64 ./run.sh cagent

# 从全新 raw 副本自动运行 BuildStorm
ARCH=loongarch64 IMAGE_MODE=copy ./run.sh buildstorm
```

脚本默认使用 `ARCH=riscv64`、`MEM=8G` 和 `IMAGE_MODE=snapshot`。RISC-V
默认使用 8 个 vCPU，LoongArch 默认使用 12 个；可通过 `SMP` 和 `MEM` 覆盖。

- `snapshot`：使用 QEMU 临时快照，退出后丢弃全部 guest 写入，适合日常调试。
- `copy`：重新创建可写 raw 工作副本，同时强制重建 `/user` 启动盘，适合需要
  干净状态且更接近 raw I/O 的性能测试。最近一次副本会保留供失败排查。

`cagent` 和 `buildstorm` 模式依赖 `expect`，会自动等待 CongCore shell、进入
`/glibc`、执行脚本并调用对应 judge。日志、评分 JSON 和可写工作镜像存放于
`.tmp/final-runs/`，不得加入版本控制。

### 使用 perf 和 QEMU `-perfmap`

Linux 宿主机上分析 QEMU 中的 guest 内核热点时，通过 `QEMU_EXTRA_ARGS` 给 QEMU
加入 `-perfmap`。`run.sh` 会把它与 `IMAGE_MODE=snapshot` 自动追加的
`-snapshot` 组合起来：

```zsh
# 终端 1：先启动 QEMU；正式采样前让 guest 进入目标 workload。
QEMU_EXTRA_ARGS=-perfmap ARCH=loongarch64 IMAGE_MODE=snapshot ./run.sh shell

# 终端 2：附加到准确的 QEMU PID，先从 30--60 秒短采样开始。
qemu_pid=$(pgrep -n -f '[q]emu-system-loongarch64')
test -n "$qemu_pid"
timeout --signal=INT --kill-after=5s 60s \
    perf record -F 99 -e cycles:u -g -p "$qemu_pid" -o perf.data
perf report -i perf.data
```

RISC-V 分析把进程名替换为 `qemu-system-riscv64`。QEMU 会生成
`/tmp/perf-${qemu_pid}.map`；在 `perf report` 完成前必须保留该文件，即使 QEMU
已经退出。确认报告可读后，只删除核对过 PID 的准确路径，禁止用宽泛通配符清理
其他进程的 perf map。

`-perfmap` 很有用，但不适合直接做耗时对比。本机实测 45 秒与 240 秒采样分别生成
约 361 MiB 和 1.3 GiB 的 map；QEMU 的翻译块生成及 libdw 符号查询进入明显热点，
响应探针也可能被拖慢。使用原则是：

- 以短采样定位候选函数，采样频率默认从 99 Hz 开始，并始终设置硬截止；
- 若 guest 探针超过既定上限，立即向 `perf` 发送 `SIGINT` 使其完整落盘，然后停止
  QEMU，不等待疑似卡死自行恢复；
- `perf record` 结束后检查 `Total Lost Samples`，丢样或损坏的 `perf.data` 不作为
  结论依据；
- 确定热点后关闭 `-perfmap`，用相同镜像状态、SMP、内存、workload 和截止时间做
  A/B；同时记录 guest 进度、探针延迟、QEMU RSS、host 可用内存和 swap；
- host 总体计数可用不带 `-perfmap` 的 `perf stat` 补充，但任何优化都必须由实际
  guest workload 的进度或完成耗时证明。

## 决赛测例

两个计分项目都使用 glibc 环境。

### CAgent

`/glibc/cagent_testcode.sh` 会启动 `simple_llm_server`，然后并发运行 10 个
CAgent 任务，覆盖：

- shell 计算和日期处理；
- CPU 与内核信息查询；
- TCP 连接状态查询；
- 文件创建、读写、目录操作、搜索和文件系统容量查询。

通过正确性校验可获得基础分；通过且耗时低于超时时间一半时可获得时间奖励。
决赛适配应优先从 CAgent 开始，因为它比 BuildStorm 更容易提供聚焦的反馈。

### BuildStorm

`/glibc/buildstorm_testcode.sh` 会：

1. 挂载 `/proc`、`/sys` 和 `/dev`；
2. 检查 `rustc` 与 `cargo`；
3. 创建、编译并运行最小 Rust 工程；
4. 编译辅助工具 `tg-xtask`；
5. 在 `/work/tgoskits` 中离线、计时编译 `arceos-helloworld`。

完整编译涉及动态链接的 glibc 程序、数百个 Rust crate、多进程与多线程、重度
文件系统访问、`mmap`、同步原语及大量系统调用。只有成功完成编译后才能获得
编译成功分和性能分。

Guest 内存至少使用 8 GiB，并始终显式传入 SMP 数量。当前 judge 源码把
RISC-V 的期望核数设为 8，把 LoongArch 的期望核数设为 12；README 中仍有
通用的 `-smp 8` 示例。正式采集成绩前必须重新核对最新 judge 和比赛规则。

禁止伪造计时结果或 `/proc/uptime`，比赛规则将此视为作弊。

## 接入原则

决赛镜像本身是完整根文件系统。`run.sh` 将它作为第一块 VirtIO 盘
`/dev/vda` 和根文件系统 `/`，另行生成只包含 CongCore 用户程序的
`user.ext4`，作为 `/dev/vdb` 挂载到 `/user`。路径解析不得在磁盘间回退；
每个路径只在最长匹配的挂载所对应的文件系统中查找。

初赛入口使用三盘布局：`system.ext4` 为 `/dev/vda` 和 `/`，`user.ext4`
为 `/dev/vdb` 和 `/user`，初赛 sdcard 为 `/dev/vdc` 和 `/mnt/oscomp`；
`/glibc` 与 `/musl` 是从 `/mnt/oscomp` 建立的 bind mount。决赛入口只使用
前述决赛根盘和 user 盘，不挂载初赛 system/test 镜像。

建议按以下阶段推进：

1. 使用一次性镜像状态启动，并进入 `/glibc`。
2. 验证动态 glibc 加载和基础 shell 命令。
3. 运行 CAgent 并使用本地 judge 评分。
4. 单独运行 BuildStorm 工具链检查。
5. 单独运行最小 Cargo 编译。
6. 以正确性为目标运行完整 BuildStorm。
7. 编译稳定成功后再进行性能分析和优化。
8. 每个完整内核修复后重新运行相关初赛/LTP 回归。

每次有意义的测试必须记录：

- 内核 commit；
- 决赛测例源码 commit；
- 镜像 SHA-256；
- QEMU 版本；
- 架构、SMP 和内存；
- 完整启动命令；
- 串口日志。

## 评分与日志

使用决赛源码中的 judge 对串口日志评分：

```zsh
python3 testsuits-for-oskernel/judge/judge_cagent-glibc.py < cagent.log
python3 testsuits-for-oskernel/judge/judge_buildstorm-glibc.py buildstorm.log
```

CAgent 满分 200。BuildStorm 包含脚本评分 180 分，以及设计与优化文档人工评分
20 分。

设计与优化文档必须保留以下证据：

- 实际失败现象或性能瓶颈；
- 根因分析；
- 可复用的内核设计与修复；
- 修改前后的环境、耗时和对比；
- AI 使用说明和完整复现步骤。

## 完成标准

一个决赛测例批次只有同时满足以下条件才算完成：

- 从干净的一次性镜像状态通过聚焦的决赛测例；
- 串口输出能够被对应的本地 judge 正确识别；
- 相关初赛/LTP 回归仍然通过；
- 已移除临时测试裁剪和调试输出；
- 已记录命令、源码与镜像版本、结果和性能证据；
- 已提出简洁的提交范围和祈使句式 commit message。
