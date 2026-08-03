<!-- This file documents the final test investigation and optimization process. Agents do not need to read or modify it unless the user explicitly asks them to. -->
#  优化日志


## 2026-07-30：先搞清楚 final 测试在测什么

请先按照Agents.md 配置好 img

### 主要有两个部分
| 项目 | 工作负载 | 主要考查能力 |
| --- | --- | --- |
| CAgent | 10 个 agent 并发通过本地 HTTP 调用“假 LLM”，再执行 shell 工具 | 进程并发、回环 TCP、HTTP/JSON、管道和信号、基础文件系统、时间与系统信息接口 |
| BuildStorm | 在完整 glibc/ext4 根文件系统中运行 Rust 工具链并编译数百个 crate | Linux ABI、动态链接、进程线程、虚拟内存、ext4/VFS、页缓存、同步与多核调度，以及整机性能 |


## 1. CAgent：主要考察一些基础系统调用是否实现

### 1.1 实际执行流程


```text
cagent_testcode.sh
  ├─ 启动 simple_llm_server，监听 127.0.0.1:8080
  └─ 并发启动 10 个 agent_lite
       ├─ 通过 TCP/HTTP POST 提交 prompt
       ├─ 收到一个固定的 bash tool call
       ├─ 通过 popen() 启动 shell 执行命令并收集输出
       └─ 再次请求服务端，得到最终回答
```

10 个测试通过 shell 后台任务同时运行，每项都有 20～35 秒超时。因此它不只
要求某条命令能单独执行，还要求内核能同时维持多个进程、管道、定时器和 TCP
连接，并正确完成 `wait`、超时终止和资源回收。
### 1.2 涉及到的系统调用

- 回环网络：`socket`、`bind`、`listen`、`accept`、`connect`、`send`、
  `recv`、`close` 和 `SO_REUSEADDR`；
- 进程和 IPC：shell 后台任务、`fork/clone`、`exec`、`wait`、`popen`、
  pipe、标准输入输出重定向；
- 时间和信号：`date` 计时、`sleep`、`timeout`、进程终止和 `SIGTERM`；
- 文件与目录操作；
- JSON 解析过程中普通的用户态内存分配和扩容。


### 1.3 10 个测试逐项对应的能力

| 测试 | fake LLM 实际选择的命令 | 脚本判定 | 主要覆盖的内核能力 |
| --- | --- | --- | --- |
| `factorial` | `echo 3628800` | 输出包含 `3628800` | 完整 agent/HTTP/shell 执行链路的基线 |
| `date` | `date -d '100 days ago' ...` | 输出包含英文星期名 | 时钟读取、日期工具运行、时区/时间文件访问 |
| `network` | `ss -tan \| grep ESTAB \| wc -l` | 输出包含数字 | TCP 状态查询接口、pipe、多个命令进程 |
| `cpu` | `nproc` | 输出包含数字 | CPU 数量或亲和性查询接口 |
| `kernel` | `uname -r` | 输出包含形如 `x.y` 的版本号 | `uname` ABI 和内核版本信息 |
| `fs-create` | `printf ... > test_file.txt` | 文件存在且内容包含 `Hello OS` | 创建、打开、截断、写入、关闭和路径解析 |
| `fs-readwrite` | 写入 1～5，再用 `awk` 求和 | 输出包含 `15` 或 `fifteen` | 文件读写、重定向、pipe/exec 和数据一致性 |
| `fs-directory` | `mkdir`、`touch` 三个文件、`ls \| wc` | 目录存在且至少有 3 个条目 | 建目录、建文件、目录遍历和元数据可见性 |
| `fs-search` | `find . -name '*.sh' \| wc -l` | 输出包含数字 | 递归路径遍历、`getdents`、`stat` 类操作 |
| `fs-usage` | `df -h /` 后提取使用率 | 输出包含数字及可选单位 | 根文件系统挂载、`statfs` 类容量查询 |

这里的“主要覆盖”不是完整 syscall 清单。`ss`、`nproc`、`date` 等程序具体调用
哪些 ABI，取决于镜像中的用户态实现；内核应该提供 Linux 兼容机制，而不是按
测试名伪造输出。

### 1.4 结论

目前我们 基本已经能cover 住 这方面的各个能力，不需要进行什么过多优化 设计 


## 2. BuildStorm：真实复杂软件构建能力测试

### Overview 
BuildStorm 完全离线，不下载依赖。它在比赛提供的 ext4 根文件系统中使用
Debian glibc 用户态、Rust nightly 工具链、Cargo 离线缓存和
`/work/tgoskits` 源码，最终从源码构建 `arceos-helloworld`。

### testcode.sh 解析 - 即 根据 这个实现完备的系统

- 首先会挂载一些路径 

```sh
mount -t proc proc /proc 2>/dev/null
mount -t sysfs sysfs /sys 2>/dev/null
mount -t devtmpfs devtmpfs /dev 2>/dev/null
```

我们需要实现profs 和 sys dev 等文件体系
在linux 中这一过程主要由systemd 进行 。
也可能把相关配置写在 /etc/fstab，再由启动脚本执行：
mount -a
我们目前的做法是：
在内核中已经实现了一套 路径特判，对于这些伪文件系统的id ，动态生成对应的值.

TODO: 实现一个完美的 真正的mount 

然后是一些环变量的设置

注意offline 设置

```sh

export PATH=/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/sbin:/usr/sbin
export HOME=/root RUSTUP_HOME=/root/.rustup CARGO_HOME=/root/.cargo
export RUSTUP_TOOLCHAIN=nightly-2026-05-28
export CARGO_NET_OFFLINE=true
```

接下来是使用uname 判断系统架构设置AXARCH 和  AXTGT：

  case "$(uname -m 2>/dev/null)" in

  - uname -m：输出机器架构，例如 loongarch64 或 riscv64。
  - $(...)：把命令输出作为 case 的匹配值。
  - 2>/dev/null：丢弃错误信息。

  匹配逻辑是：

  - 如果是 loongarch64：
      - AXARCH=loongarch64
      - AXTGT=loongarch64-unknown-linux-musl

  - 如果是 riscv64：
      - AXARCH=riscv64
      - AXTGT=riscv64gc-unknown-linux-musl

 这两个变量后面分别用于：  
 rm -rf "target/$AXTGT"  
 删除对应架构的旧编译产物，确保进行重新编译。  
 cargo xtask arceos build ... --arch "$AXARCH"  
 告诉 ArceOS/tg-xtask 编译哪个架构。

然后是运行rustc cargo 的检查 

```sh
if rustc --version && cargo --version; then
    echo "BUILDSTORM_TOOLCHAIN ok"
else
    echo "BUILDSTORM_TOOLCHAIN fail"
fi

```
运行这个需要 基本的elf 执行能力 我们已经有了 
下面是一个小的new + build + 运行
参数意思分别是

  - --vcs none：不创建 Git 仓库。
  - >/dev/null：丢弃标准输出。
  - 2>&1：把标准错误也重定向到 /dev/null。

```sh
rm -rf /tmp/minibuild
if cargo new --vcs none /tmp/minibuild >/dev/null 2>&1 \
   && ( cd /tmp/minibuild && cargo build >/dev/null 2>&1 ) \
   && [ "$(/tmp/minibuild/target/debug/minibuild)" = "Hello, world!" ]; then
    echo "BUILDSTORM_MINIBUILD ok"
else
    echo "BUILDSTORM_MINIBUILD fail"
fi

cd /work/tgoskits 2>/dev/null || {
    echo "BUILDSTORM_COMPILE mode=multi ok=false elapsed_s=0 cores=$(nproc) bytes=0 arch=$AXARCH"
    echo "#### OS COMP TEST GROUP END buildstorm ####"
    exit 1
}

rm
```

下面进入正式的大头任务 首先检测一下当前环境

```sh
cd /work/tgoskits 2>/dev/null || {
    echo "BUILDSTORM_COMPILE mode=multi ok=false elapsed_s=0 cores=$(nproc) bytes=0 arch=$AXARCH"
    echo "#### OS COMP TEST GROUP END buildstorm ####"
    exit 1
}
```

```sh
T0=$(cut -d' ' -f1 /proc/uptime 2>/dev/null)
{ timeout 14400 cargo xtask arceos build -p arceos-helloworld --arch "$AXARCH" 2>&1; \
  echo $? > /work/.build.rc; } | tee /work/buildstorm.build.out
RC=$(cat /work/.build.rc 2>/dev/null || echo 1); rm -f /work/.build.rc
T1=$(cut -d' ' -f1 /proc/uptime 2>/dev/null)
ELAPSED=$(awk "BEGIN{printf \"%.2f\", (\"$T1\"+0)-(\"$T0\"+0)}" 2>/dev/null); [ -z "$ELAPSED" ] && ELAPSED=0

ART=$(find target -type f \( -name 'arceos-helloworld' -o -name 'helloworld' \) 2>/dev/null | head -1)
BYTES=0
[ -n "$ART" ] && BYTES=$(wc -c <"$ART")

if [ "$RC" -eq 0 ] && [ -n "$ART" ] && [ "$BYTES" -ge 500000 ]; then
    echo "BUILDSTORM_COMPILE mode=multi ok=true elapsed_s=$ELAPSED cores=$(nproc) bytes=$BYTES arch=$AXARCH"
else
    echo "BUILDSTORM_COMPILE mode=multi ok=false rc=$RC elapsed_s=$ELAPSED cores=$(nproc) bytes=$BYTES arch=$AXARCH"
    echo "----- buildstorm.build.out tail -----"
    tail -25 /work/buildstorm.build.out 2>/dev/null
fi

```

过程比较长 我们逐一来分析一下
## 1. 记录编译开始时间
  T0=$(cut -d' ' -f1 /proc/uptime 2>/dev/null)
  Linux 的 /proc/uptime 通常类似：
  1234.56 9876.54
  第一个字段是系统启动后经过的秒数。
  这里：
  - -d' '：以空格作为分隔符。
  - -f1：取第一个字段。
  - $(...)：捕获命令输出。
  - 2>/dev/null：隐藏错误。
  最终可能得到：
  T0=1234.56
  使用 /proc/uptime 而不是伪造计时结果，也是比赛规则要求的一部分。
  ## 2. 执行完整编译并记录日志

  { timeout 14400 cargo xtask arceos build \
      -p arceos-helloworld \
      --arch "$AXARCH" 2>&1; \
    echo $? > /work/.build.rc; } |
  tee /work/buildstorm.build.out
## 编译命令

  cargo xtask arceos build \
      -p arceos-helloworld \
      --arch "$AXARCH"

  含义是：

  - cargo xtask：运行项目自定义的 xtask 构建工具。
  - arceos build：调用 xtask 的 ArceOS 构建子命令。
  - -p arceos-helloworld：构建 arceos-helloworld 包。
  - --arch "$AXARCH"：指定 riscv64 或 loongarch64。

限制时间4 小时 

## 3. 时间测量
T1=$(cut -d' ' -f1 /proc/uptime 2>/dev/null)
编译完成后再次读取系统运行时间。
例如：
T0=1234.56
T1=1354.91
然后计算差值：
```sh
ELAPSED=$(awk \
"BEGIN{printf \"%.2f\", (\"$T1\"+0)-(\"$T0\"+0)}" \
2>/dev/null)
```
结果保留两位小数： ELAPSED=120.35
如果 awk 不存在、运行失败或者没有产生输出： [ -z "$ELAPSED" ] && ELAPSED=0 将耗时设置为 0。 需要注意：如果 /proc/uptime 读取失败但 awk 仍能运行，空字符串可能被当作 0，因此这一保 护并不能识别所有异常计时情况。

## 4.产物检测

  ART=$(find target -type f \
      \( -name 'arceos-helloworld' -o -name 'helloworld' \) \
      2>/dev/null |
      head -1)`
最终例如：
  ART=target/riscv64gc-unknown-linux-musl/release/arceos-helloworld
  如果没有找到，ART 是空字符串。
## 5. 产物判断大小
BYTES=0
  [ -n "$ART" ] && BYTES=$(wc -c <"$ART")

  if [ "$RC" -eq 0 ] \
     && [ -n "$ART" ] \
     && [ "$BYTES" -ge 500000 ]; then

  需要同时满足三个条件：
  1. 编译命令退出码是 0。
  2. 找到了目标产物。
  3. 产物大小至少是 500000 字节。
## 6. 成功时输出：

  BUILDSTORM_COMPILE mode=multi ok=true \
  elapsed_s=120.35 cores=8 bytes=1357824 arch=riscv64

  字段包括：

  - mode=multi：多核模式。
  - ok=true：编译成功。
  - elapsed_s：耗时。
  - cores=$(nproc)：系统报告的 CPU 数量。
  - bytes：产物大小。
  - arch：目标架构。

  这行会被 BuildStorm judge 解析。

## 7. 失败情况

  任何一个成功条件不满足，就输出：

  BUILDSTORM_COMPILE mode=multi ok=false \
  rc=$RC elapsed_s=$ELAPSED cores=$(nproc) \
  bytes=$BYTES arch=$AXARCH

  然后显示完整日志的最后 25 行：

  echo "----- buildstorm.build.out tail -----"
  tail -25 /work/buildstorm.build.out 2>/dev/null

  这样不会把整份日志重复打印一遍，但能显示最接近失败位置的错误，例如：

  - Rust 编译错误
  - 链接器错误
  - 内存不足
  - syscall 未实现
  - Cargo 子进程异常退出
  - 超时信息

  完整日志仍保存在：

  /work/buildstorm.build.out

  ## 10. 输出结束标记并刷新磁盘

  echo "#### OS COMP TEST GROUP END buildstorm ####"
  sync
<!---->
<!-- ### 2.1 测试分成三层 -->
<!---->
<!-- | 阶段 | 实际动作 | 直接证明的能力 | -->
<!-- | --- | --- | --- | -->
<!-- | 工具链检查 | 执行 `rustc --version`、`cargo --version` | 动态 glibc 程序和大型 ELF 能加载；基础文件、内存与进程 ABI 可用 | -->
<!-- | 最小构建 | `cargo new`、`cargo build`，再运行 `Hello, world!` | Cargo → rustc → linker → 新 ELF 执行的完整最小链路 | -->
<!-- | 完整构建 | 预编译 `tg-xtask`，再计时构建 `arceos-helloworld` | 数百 crate、多进程多线程、重度文件 I/O、`mmap`、同步和多核调度的综合正确性与性能 | -->
<!---->
<!-- 脚本会尝试挂载 `/proc`、`/sys` 和 `/dev`。其中 `/proc/uptime` 是编译计时的 -->
<!-- 直接依赖，`nproc` 的结果会写入评分记录；sysfs 和 devtmpfs 的挂载失败被脚本 -->
<!-- 静默忽略，但工具链运行过程中仍可能间接依赖相应设备和系统信息。 -->
<!---->
<!-- ### 2.2 BuildStorm 具体考查的内核子系统 -->
<!---->
<!-- #### Linux 用户态 ABI 与动态链接 -->
<!---->
<!-- - 正确加载动态 glibc ELF、动态链接器和共享库； -->
<!-- - ELF 装载、辅助向量、TLS、环境变量和参数传递； -->
<!-- - glibc、rustc、cargo、linker、shell、`find`、`tee` 等真实程序所需 syscall； -->
<!-- - 正确的错误码、文件描述符和资源生命周期。 -->
<!---->
<!-- 这不是“能启动一个静态 BusyBox”就能通过的测试。 -->
<!---->
<!-- #### 进程、线程与 IPC -->
<!---->
<!-- - 大量进程创建、`exec`、退出和 `wait`； -->
<!-- - Rust/Cargo 并行任务和多线程运行； -->
<!-- - pipe、重定向、`dup`、轮询/等待和信号； -->
<!-- - `timeout` 的终止语义及异常退出后的资源回收； -->
<!-- - futex 等用户态同步原语的阻塞、唤醒与并发正确性。 -->
<!---->
<!-- 任何偶发丢唤醒、僵尸进程泄漏、文件描述符泄漏或等待语义错误，都可能在长时间 -->
<!-- 构建中放大为卡死或随机失败。 -->
<!---->
<!-- #### 虚拟内存 -->
<!---->
<!-- - `mmap`、`munmap`、`mprotect`、`brk` 和文件映射； -->
<!-- - ELF 映射、线程栈、TLS、缺页处理和地址空间切换； -->
<!-- - fork/exec 路径中的地址空间复制或写时复制； -->
<!-- - 8 GiB guest 内存下的页分配、回收和长时间稳定性； -->
<!-- - 文件页缓存与 `mmap`/普通读写之间的一致性。 -->
<!---->
<!-- rustc 和 linker 是内存密集型程序；小程序可运行并不代表这里不会暴露越界、 -->
<!-- 映射、回收或并发缺陷。 -->
<!---->
<!-- #### VFS、ext4 与块 I/O -->
<!---->
<!-- - 决赛 ext4 镜像作为 `/` 正确读写挂载； -->
<!-- - 绝对/相对路径、当前目录、挂载点和最长前缀解析； -->
<!-- - 高频 `open/read/write/close/stat/getdents`； -->
<!-- - 文件和目录的创建、删除、重命名、截断、递归遍历； -->
<!-- - symlink、权限、时间戳以及 Cargo 常用的锁文件语义； -->
<!-- - 页缓存、脏页回写、元数据更新和 `sync`； -->
<!-- - 大量小文件和目录项操作的吞吐与锁粒度。 -->
<!---->
<!-- BuildStorm 会修改 `/tmp`、`/work` 和 Cargo target，因此必须使用 QEMU -->
<!-- snapshot 或一次性 raw 副本，不能直接写唯一的基准镜像。 -->
<!---->
<!-- #### 多核调度与性能 -->
<!---->
<!-- - RISC-V 本地配置使用 8 vCPU，LoongArch 本地配置使用 12 vCPU； -->
<!-- - 多核任务能真正并行，而不是只让一个核工作； -->
<!-- - runnable task 分配、负载均衡、唤醒路径和锁竞争； -->
<!-- - syscall、上下文切换、TLB/地址空间切换的总体开销； -->
<!-- - ext4 元数据、页缓存、块设备队列和全局锁的扩展性。 -->
<!---->
<!-- 这部分测的是整条构建关键路径的墙钟时间，不能只优化某一个 syscall。常见真实 -->
<!-- 瓶颈会集中在文件元数据、页缓存和回写、进程创建、futex 唤醒、调度负载均衡 -->
<!-- 以及过粗的全局锁。 -->
<!---->
<!-- ### 2.3 完整构建的成功条件和计时边界 -->
<!---->
<!-- 脚本的关键步骤是： -->
<!---->
<!-- 1. 删除 `target/<目标架构>`，保证目标架构产物重新构建； -->
<!-- 2. 不计时地执行 `cargo build -p tg-xtask`； -->
<!-- 3. 从 `/proc/uptime` 读取开始时间； -->
<!-- 4. 执行 -->
<!--    `cargo xtask arceos build -p arceos-helloworld --arch <arch>`； -->
<!-- 5. 通过 `tee` 保存完整构建输出； -->
<!-- 6. 再读 `/proc/uptime` 计算耗时； -->
<!-- 7. 查找产物并检查其大小至少为 500,000 字节； -->
<!-- 8. 最后执行 `sync`。 -->
<!---->
<!-- 只有命令退出码为 0、找到产物且产物不少于 500,000 字节时，脚本才打印 -->
<!-- `ok=true`。内部编译命令超时为 14,400 秒；本地 `run.sh` 的 BuildStorm 外层 -->
<!-- 默认测试超时为 18,000 秒。 -->
<!---->
<!-- 计时只覆盖第 4 步，不包括 `tg-xtask` 的预编译、环境挂载和最后的 `sync`。 -->
<!-- 因此性能对比必须使用相同镜像初始状态、相同架构/SMP/内存和相同脚本，不能把 -->
<!-- 不同缓存状态的结果直接比较。 -->
<!---->
<!-- ### 2.4 BuildStorm 评分实际含义 -->
<!---->
<!-- 本地 judge 的 180 个脚本分为： -->
<!---->
<!-- - `rustc` 和 `cargo` 可运行：8 分； -->
<!-- - 最小 Cargo 工程可构建并运行：12 分； -->
<!-- - 完整构建成功：40 分； -->
<!-- - 完整构建性能：120 分。 -->
<!---->
<!-- 性能分公式为： -->
<!---->
<!-- ```text -->
<!-- 120 × clamp((2 × B - t) / B, 0, 1) -->
<!-- ``` -->
<!---->
<!-- 其中 `t` 是 guest 从 `/proc/uptime` 得到的编译耗时，`B` 是 Linux 基线。 -->
<!-- `t <= B` 得 120 分，`t >= 2B` 得 0 分，中间线性递减。 -->
<!---->
<!-- 本地 commit 中的自测基线是： -->
<!---->
<!-- | 架构 | 基线 B | judge 期望核数 | -->
<!-- | --- | ---: | ---: | -->
<!-- | RISC-V64 | 4655.23 秒 | 8 | -->
<!-- | LoongArch64 | 6223.0 秒 | 12 | -->
<!---->
<!-- 这些常量只用于当前本地 judge 自检，平台正式 judge 可能不同。当前本地 judge -->
<!-- 发现核数不符时只打印警告，不直接扣分，但正式测试仍必须使用规定 SMP。 -->
<!---->
<!-- 另外 20 分由人工评审设计与优化文档： -->
<!---->
<!-- - 问题或瓶颈定位与根因分析：6 分； -->
<!-- - 修复或优化的设计与实现：6 分； -->
<!-- - 修改前后时间、加速比等实验对比：4 分； -->
<!-- - AI 使用说明和可复现步骤：4 分。 -->
<!---->
<!-- 本地 judge 在完整构建失败时仍会分别计算前两项环境分，但本地 `run.sh` 会把 -->
<!-- “工具链、最小构建、完整构建”任一正确性项失败视为整次运行失败。正式报告应 -->
<!-- 同时记录 JSON 分数和运行是否通过，不能只看总分。 -->
<!---->
<!-- ### 2.5 BuildStorm 不直接测试什么 -->
<!---->
<!-- - 全程设置 `CARGO_NET_OFFLINE=true`，所以不测试外网下载能力； -->
<!-- - 不要求完整复刻 Linux 内核内部实现，只要求用户可观察语义、ABI、并发结果 -->
<!--   和性能满足测试； -->
<!-- - 一次成功不等于没有竞态，仍需从干净镜像重复运行并做初赛/LTP 回归； -->
<!-- - 编译耗时是综合指标，不能仅凭总时间判断具体瓶颈，优化前必须用日志或性能 -->
<!--   数据定位。 -->

## 2026-07-31：参考 Linux 拆除 ext4/块 I/O 全局串行路径

本批次只处理 P0 的 ext4、VirtIO 块 I/O 和与硬中断唤醒直接相关的调度并发；
完整 VFS 节点化和挂载图迁移不在本批次内。按要求未运行 BuildStorm。

### 基线、源码和测试资产

- 父仓库 HEAD：`38e32c422eafa4e92c4252832dfce9a500532be3`，含本批次未提交修改。
- 当前 OS HEAD：`7dd9c5c875a9f9b037de82eae492e7dbff75a86a`，含本批次未提交修改。
- 对照基线 OS HEAD：
  `/Users/bytedance/projects/OS_Workspace-ext4-fix/os` 的
  `e78c5e16441b6cef297baaf5f483270f24fca0f5`。
- final 测试源码：`final-2026`，
  `1eac61d3becaa592c8ef12a7535f0ec6bb9e3e36`。
- 本地 Linux 参考树：`exampleOs/linux`，
  `fc02acf6ac0ccde0c805c2daa9148683cdd01ba8`。
- QEMU：11.0.3；RISC-V64，8 vCPU，2 GiB，snapshot 模式。
- IOZone 根镜像：
  `.tmp/iozone/iozone-root.img`，4 GiB，
  SHA-256 `c0cb4e209e0aa243af72c599c3e34a679ff1f195ac46789e054b77eac3bf453d`。
  这是只用于聚焦测试、加入了 `/glibc/iozone` 的临时镜像，不是 14 GiB
  决赛基准镜像。
- 当前 `/user` 镜像 SHA-256：
  `076d153dc2db1736c09ce9d97948afc9d1b26c3863436eb80cb9ab6c2aa2ab40`。
- 基线工作树 `/user` 镜像 SHA-256：
  `fc7eccd2ddc5e87c2c70116bd3f63dcf14e9ffd38b6d96b37e756f7c6441890e`。
  两个工作树必须使用各自构建的 `/user` 镜像才能启动对应内核；IOZone
  数据文件都位于同一个临时根镜像的 `/glibc`。
- 两次 A/B 均在宿主机已有其他 QEMU 负载时顺序执行，因此只比较这组同负载数据；
  更早、宿主负载不同的 70.38 秒历史结果不纳入加速比。
- 官方 14 GiB RISC-V/LoongArch 基准镜像未被写入或替换。

### Linux 对照和本项目实现

没有逐行移植 Linux，而是保留 CongCore 架构并复用 Linux 已验证的锁域、IRQ
语义、内存序和对象生命周期：

| 本项目机制 | 本地 Linux 对照 | 落地方式 |
| --- | --- | --- |
| VirtIO 提交/完成串行化 | `drivers/block/virtio_blk.c` 的 `virtio_queue_rq()`、`virtblk_done()` | 提交和硬中断完成共用 irq-save 队列锁；请求元数据由驱动持有到 used ring 返回 |
| VirtQueue 回调和 kick | `drivers/virtio/virtio_ring.c` 的 callback suppression、`virtqueue_kick_prepare()` | IRQ 与有界轮询共用完成函数；轮询消费完成后再确认中断；保留既有 LoongArch 强制 kick 兼容边界 |
| 有界块轮询 | `block/blk-mq.c` 的 `blk_hctx_poll()` | 最多 64 次短轮询，遇到 reschedule 或预算耗尽后无信号地协作让出，再继续等待 DMA 完成 |
| 块请求生命周期 | Linux `bio`/request 在完成前 pin 页面 | 同步调用者在完成前不能返回；请求表持有 `Arc`，数据直接使用调用者稳定缓冲区，删除每个 4 KiB 请求的二次分配和复制 |
| waitqueue | `kernel/sched/wait.c` 的 prepare/recheck/wake 顺序 | 条件检查、入队和唤醒使用 irq-save 元数据锁，阻止“检查后、睡眠前”丢唤醒 |
| SMP 唤醒交接 | `kernel/sched/core.c::try_to_wake_up()` | 每任务 transition lock 对应 `p->pi_lock`，用 acquire/release 的 `on_cpu` 交接决定立即入队或延迟唤醒 |
| inode 锁粒度 | Linux `inode::i_rwsem`、`s_vfs_rename_mutex`、`lock_two_nondirectories()` | `(device_id, inode_num)` 对应稳定读写信号量；多 inode 按稳定键排序；跨目录 rename 另取保守的 topology mutex |
| page-cache 缺页合并 | `mm/filemap.c` 的同页缺页协调 | block cache 使用 `Loading/Ready` single-flight，同一冷块只提交一次 I/O |
| 回写锁域 | Linux page cache/writeback 不在全局映射锁内等待设备 | 冷读、LRU 淘汰回写、`sync_all` 均移出全局 cache-manager 锁；generation 防止并发写被旧回写误标为干净 |
| 有界预读 | `mm/readahead.c` | 只在已确认连续的 ext4 extent 内预读，单次最多 32 个 4 KiB 块，即 128 KiB |

实现边界：

- CongCore 尚无完整 blk-mq timeout/reset/recovery；30 秒阈值只诊断，不能在 DMA
  仍拥有缓冲区时伪造超时返回。
- LoongArch vendor 原有同步路径已经强制 kick；本批次的非阻塞路径沿用该兼容行为。
  Linux 无按架构强制 kick 的特判，后续应单独修正 vendor 的 event-index 判定并做
  LoongArch 运行态回归，不能把这个兼容项称为 Linux 设计。
- 当前 topology mutex 比 Linux 的 per-superblock `s_vfs_rename_mutex` 更保守，但
  只覆盖跨目录 rename，不再串行化普通 lookup/read/write。
- 本批次未实现 Linux RCU pathwalk、完整 page cache 或多硬件队列。

### 修改文件

本批次共涉及 54 个文件；未计入用户原有的
`fix-per-day/7.30-vfs-core.md` 删除、`.DS_Store` 和
`fix-per-day/7.30-mutli-core-for-loongarhc.md`。

`ext4-fs/`（5）：

```text
ext4-fs/src/block_cache.rs
ext4-fs/src/block_dev.rs
ext4-fs/src/ext4.rs
ext4-fs/src/lib.rs
ext4-fs/src/vfs.rs
```

`vendor/virtio-drivers-pci/`（4）：

```text
vendor/virtio-drivers-pci/src/device/blk.rs
vendor/virtio-drivers-pci/src/lib.rs
vendor/virtio-drivers-pci/src/queue.rs
vendor/virtio-drivers-pci/src/transport/pci/bus.rs
```

`os/`（43，其中 4 个新文件）：

```text
os/src/arch/loongarch64/csr_defs.rs
os/src/arch/loongarch64/irq.rs                         [new]
os/src/arch/loongarch64/mod.rs
os/src/arch/loongarch64/trap/handler.rs
os/src/arch/riscv64/irq.rs                             [new]
os/src/arch/riscv64/mod.rs
os/src/arch/riscv64/trap/handler.rs
os/src/config.rs
os/src/drivers/block/async_queue.rs                    [new]
os/src/drivers/block/mod.rs
os/src/drivers/block/virtio_blk.rs
os/src/fs/ext4/mod.rs
os/src/fs/fanotify.rs
os/src/fs/inode.rs
os/src/fs/mod.rs
os/src/fs/procfs/magic_link.rs
os/src/lib.rs
os/src/main.rs
os/src/sync.rs                                         [new]
os/src/syscall/filesystem/ctl.rs
os/src/syscall/filesystem/ctx_utils.rs
os/src/syscall/filesystem/dir.rs
os/src/syscall/filesystem/fanotify.rs
os/src/syscall/filesystem/fd_utils.rs
os/src/syscall/filesystem/inode_utils.rs
os/src/syscall/filesystem/io.rs
os/src/syscall/filesystem/mod.rs
os/src/syscall/filesystem/mount_utils.rs
os/src/syscall/filesystem/open_close.rs
os/src/syscall/filesystem/path_utils.rs
os/src/syscall/filesystem/perm_utils.rs
os/src/syscall/filesystem/stat.rs
os/src/syscall/filesystem/stat_utils.rs
os/src/syscall/memory/mmap.rs
os/src/syscall/memory/mod.rs
os/src/syscall/misc/module.rs
os/src/syscall/net/unix.rs
os/src/syscall/process/exec.rs
os/src/syscall/process/mod.rs
os/src/task/manager.rs
os/src/task/manager/fair.rs
os/src/task/processor.rs
os/src/task/task_block.rs
```

聚焦测试资产和记录（2）：

```text
testsuits-final/tools/run_iozone_focus.sh
testsuits-final/record.md
```

### IOZone A/B 结果

工作负载每轮依次执行：

```sh
iozone -t 4 -i 0 -i 1 -r 1k -s 4m
iozone -t 4 -i 0 -i 2 -r 1k -s 4m
```

连续三轮的 guest `/proc/uptime`：

| 实现 | 第 1 轮 | 第 2 轮 | 第 3 轮 | 中位数 |
| --- | ---: | ---: | ---: | ---: |
| 基线：全局 `EXT4_LOCK` | 75.72 s | 81.13 s | 81.88 s | 81.13 s |
| 当前：inode 锁 + Linux 风格块完成 + zero-copy | 76.19 s | 76.23 s | 76.77 s | 76.23 s |

当前中位耗时缩短 4.90 秒，即 6.04%。三轮 sequential/random 命令均返回 0。
基线虽然 IOZone 总返回码为 0，但三个阶段出现 `Min xfer = 0`；当前全部 worker
的 `Min xfer` 非零。

各轮 `Children see throughput` 的中位数：

| 四 worker 阶段 | 基线 kB/s | 当前 kB/s | 变化 |
| --- | ---: | ---: | ---: |
| sequential initial writers | 2389.42 | 1948.13 | -18.47% |
| sequential rewriters | 3433.32 | 5174.99 | +50.73% |
| sequential readers | 5223.11 | 7431.32 | +42.28% |
| sequential re-readers | 5316.99 | 8363.10 | +57.29% |
| random initial writers | 2338.48 | 1952.75 | -16.49% |
| random rewriters | 2880.76 | 5246.59 | +82.13% |
| random readers | 1795.68 | 1841.51 | +2.55% |
| random writers | 1629.26 | 1978.59 | +21.44% |

因此当前收益主要来自重写、命中读取和并发完成；首次创建写仍回退约 16–18%。
后续应继续分析 ext4 位图/目录元数据分配和请求对象分配，不能用本次总耗时改善掩盖
该局部回退。

复现模板：

```zsh
# 当前工作树
bash tools/run_iozone_focus.sh \
  /Users/bytedance/projects/OS_Workspace \
  .tmp/iozone/iozone-root.img \
  .tmp/iozone/linux-lock-zero-copy.log

# 对照工作树；先构建对应内核，再使用它自己的 /user 镜像
IOZONE_SKIP_BUILD=1 \
IOZONE_USER_IMG=/Users/bytedance/projects/OS_Workspace-ext4-fix/ext4-fs-packer/target/user.ext4 \
bash tools/run_iozone_focus.sh \
  /Users/bytedance/projects/OS_Workspace-ext4-fix \
  .tmp/iozone/iozone-root.img \
  .tmp/iozone/ext4-lock-concurrent-host-baseline-correct-image.log
```

日志和脚本 SHA-256：

- `.tmp/iozone/ext4-lock-concurrent-host-baseline-correct-image.log`：
  `b0d1f53fcf6b38293d02ac53acf6cbbb14752a11846f42f845712fa55a0128b1`
- `.tmp/iozone/linux-lock-zero-copy.log`：
  `0e4868b705d31c4982562789db98fe70d5319c46f5258a4018d581d274c2d46d`
- `tools/run_iozone_focus.sh`：
  `7f8ed8247335966bc17ae74c7ed1613a96d141ab5f53b8ba3d50a52e98cfdc85`

### 构建和单元验证

- RISC-V：
  `cargo check --manifest-path ../os/Cargo.toml
  --target riscv64gc-unknown-none-elf --offline` 通过。
- LoongArch：
  `cargo check --manifest-path ../os/Cargo.toml
  --target loongarch64-unknown-none-softfloat --offline` 通过。
- ext4：8/8 单元测试通过，包括 single-flight、管理器锁外冷读/回写、
  generation 并发写保护、LRU 淘汰和有界预读。
- VirtIO：24/24 单元测试、8/8 文档测试通过。vendor 测试专用
  `transport/fake.rs` 与仓库既有 `deny(missing_docs)` 冲突，测试命令使用
  `RUSTFLAGS=--cap-lints=warn`；OS 的普通静态构建未降低 lint。
- 本批次修改文件的 `rustfmt --check` 通过。
- `bash -n tools/run_iozone_focus.sh` 通过。
- 父仓库与 OS 子仓库 `git diff --check` 通过。
- 当前 IOZone 日志无 VirtIO stall/error、panic 或 deadlock 诊断。
- 未运行 BuildStorm、unixbench、libcbench 或完整 CAgent。
