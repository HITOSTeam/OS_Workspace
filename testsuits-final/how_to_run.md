# 这个文件主要包含调试 开发 的有用指导，包括proc perfmap 等工具的使用

## expect 
AI 使用了大量expect 脚本 来跟内核的shell  进行互动。

`expect` 基于 Tcl 语言，通过"等待输出→发送输入"的模式自动化交互式程序。下面按模块讲解。

## 一、核心命令

| 命令 | 作用 |
|------|------|
| `spawn <cmd>` | 启动子进程，后续 expect/send 针对该进程 |
| `expect <pattern>` | 等待子进程输出匹配该 pattern（支持通配符/正则） |
| `send <string>` | 向子进程发送字符串，`\r` 表示回车 |
| `exp_continue` | 在 expect 块内重新执行该 expect（循环等待） |
| `expect eof` | 等待子进程结束 |
| `wait` | 等待子进程退出并返回状态 |
| `interact` | 将控制权交还给用户（手动操作） |
| `close` | 关闭子进程连接 |

## 二、基础示例（SSH 自动登录）

```tcl
#!/usr/bin/expect -f

set timeout 30            ;# 全局超时（秒），-1 表示永不超时
set host "192.168.1.10"
set user "root"
set password "123456"

spawn ssh $user@$host

expect {
    "yes/no"   { send "yes\r"; exp_continue }   ;# 首次连接确认
    "password:" { send "$password\r" }
}

expect "# "               ;# 等待命令提示符
send "ls /tmp\r"
expect "# "
send "exit\r"
expect eof
```

运行：`chmod +x demo.exp && ./demo.exp`

## 三、expect 多分支匹配

```tcl
expect {
    "password:"   { send "$pwd\r" }
    "refused"     { puts "连接被拒绝"; exit 1 }
    timeout       { puts "超时"; exit 2 }
    eof           { puts "进程已退出"; exit 3 }
}
```
- 匹配按顺序进行，命中第一个即停止。
- `timeout` / `eof` 是两个特殊分支名。
- `exp_continue` 让 expect 重新等待，常用于处理重复提示。

## 四、参数化（从命令行传参）

```tcl
#!/usr/bin/expect -f
set host   [lindex $argv 0]
set user   [lindex $argv 1]
set passwd [lindex $argv 2]

spawn ssh $user@$host
expect "password:"
send "$passwd\r"
interact                  ;# 登录后交还控制权给用户
```
调用：`./login.exp 192.168.1.10 root 123456`

`$argv` 是参数列表，`$argc` 是参数个数，`$argv0` 是脚本名。

## 五、超时控制

```tcl
set timeout 20            ;# 全局
expect {
    -timeout 5            ;# 仅本块局部超时
    "ready:" { send "go\r" }
    timeout  { puts "等不到 ready" }
}
```

## 六、模式匹配语法

```tcl
expect "password:"              ;# glob 通配（默认），* ? []
expect -re "^PASS:\[0-9\]+$"    ;# 正则，需用 -re
expect -ex "100%"               ;# 精确匹配，不做通配/正则
expect -re "(\[0-9]+)%";        ;# 捕获组
puts "$expect_out(0,string)"    ;# 整个匹配
puts "$expect_out(1,string)"    ;# 第1个捕获组
```

## 七、循环处理：exp_continue

```tcl
spawn telnet $host
expect "login: "
send "$user\r"
expect {
    "Password:" { send "$pwd\r"; exp_continue }
    "$ "        { }
    timeout     { puts "登录失败"; exit 1 }
}
# 后续命令...
```

## 八、与 shell 脚本结合（单行 expect）

```bash
#!/bin/bash
expect -c '
  spawn ssh root@1.2.3.4 "uptime"
  expect "password:"
  send "123456\r"
  expect eof
'
```

也可以在 shell 里用 here-doc：

```bash
expect <<'EOF'
set timeout 10
spawn ssh root@1.2.3.4 "uname -a"
expect "password:"
send "123456\r"
expect eof
EOF
```

## 九、常见模式速查

**1. 批量改密码：**
```tcl
spawn passwd $user
expect "New password:"
send "$newpwd\r"
expect "Retype:"
send "$newpwd\r"
expect eof
```

**2. scp 传文件：**
```tcl
spawn scp file.tar.gz $user@$host:/tmp/
expect "password:"
send "$pwd\r"
expect eof
```

**3. 登录后交互再交还用户：**
```tcl
spawn ssh $user@$host
expect "password:"
send "$pwd\r"
expect "$ "
interact
```

## 十、调试技巧

- `expect -d demo.exp` 开启调试输出，能看到每次匹配的缓冲内容。
- `log_file -a session.log` 将整个交互记录到文件。
- 输出被缓冲看不到时，子进程加 `stty -echo` 或用 `unbuffer`（同 expect 包附带）：
  ```bash
  unbuffer ./myprog | grep xxx
  ```
- 用 `autoexpect` 录制脚本：`autoexpect -f demo.exp`，操作完成后 exit，自动生成 demo.exp。

## 十一、注意点

1. `send` 的 `\r` 是回车（CR），不要用 `\n`，终端需要 CR。
2. expect 默认是 glob 模式，遇到含 `*?[` 的字符串要用 `-ex` 或 `-re` 并转义。
3. `timeout` 默认 10 秒；长命令（如大文件传输）记得调大或设为 -1。
4. expect 会读取子进程输出到缓冲，匹配后消费掉已匹配部分；没匹配的会留存等待下次。
5. 密码明文写在脚本里有风险，建议用环境变量或读取受限权限文件。


## proc
Linux 的 `/proc` 是内核暴露的进程/系统信息伪文件系统，是性能分析的基础数据源。下面按"看什么、怎么看、怎么用"讲解。

## 一、定位：/proc 能做什么

- **进程级**：CPU、内存、IO、线程、调用栈、文件描述符、信号等
- **系统级**：CPU、内存、磁盘、网络、中断、上下文切换等
- **实时快照**：大部分是实时刷新的文本/二进制文件，用 `cat`/`awk` 即可读取
- **配合工具**：`top`/`ps`/`free`/`vmstat`/`pidstat` 等本质上都是读 `/proc`

## 二、进程级分析（核心：/proc/[pid]/）

假设目标 PID = 1234。

### 1. CPU 占用来源

| 文件 | 内容 | 用途 |
|------|------|------|
| `stat` | 进程状态、utime/stime（用户/内核 jiffies）、启动时间 | 计算 CPU% |
| `status` | 人可读的 State、VmRSS、Threads、 voluntary/involuntary ctxt | 快速概览 |
| `task/` | 每个线程一个子目录，内含各线程 stat | 分析多线程谁在占 CPU |
| `sched` | 调度信息、运行队列等待、切换次数 | 判断是否被频繁抢占 |

**计算 CPU%（两次采样）：**
```bash
awk '/^utime|^stime|^starttime/{print}' /proc/1234/stat
```
- utime/stime 单位是 clock_t（通常 `sysconf(_SC_CLK_TCK)` = 100，即 1/100 秒）。
- CPU% = Δ(utime+stime) / (采样间隔 × 时钟频率 × CPU核数) × 100。

**找最忙的线程：**
```bash
for t in /proc/1234/task/*; do
  awk '{print $1, $14+$15}' "$t/stat"
done
```
$14=utime, $15=stime。

**看进程状态（是否在 D 不可中断睡眠，典型 IO 阻塞）：**
```bash
awk '/^State/{print $2}' /proc/1234/status   # R/S/D/Z/T
```
`D` 状态多 → IO 瓶颈；配合 `stack` 看阻塞点。

### 2. 调用栈（定位卡在哪）

```bash
cat /proc/1234/stack          ;# 内核栈（需内核开启 CONFIG_STACKTRACE）
cat /proc/1234/task/<tid>/stack
```
- 显示内核态调用链，判断是否卡在 `io_schedule`、`mutex_lock`、`schedule` 等。
- 用户态栈需用 `gstack 1234` / `perf` / `gdb -p 1234`，/proc 不直接提供。

### 3. 内存分析

| 文件 | 关键字段 |
|------|---------|
| `status` | VmPeak(峰值) / VmSize(虚拟) / VmRSS(物理) / VmHWM(RSS峰值) / VmData(堆) / VmStk(栈) / VmExe(代码段) / VmLib(库) / VmPTE |
| `statm` | size/resident/shared/text/lib/dirt 页数（页=4KB） |
| `smaps` | 每个 mmap 区段的 RSS/PSS/Anonymous/Swap/HugePages，**含 PSS（按比例分摊）**——最准的内存归因 |
| `smaps_rollup` | smaps 汇总值，读取代价小 |
| `maps` | 地址空间布局，看映射了哪些 so/heap/stack |
| `oom_score` / `oom_score_adj` | OOM 杀手打分，判断谁最容易被杀 |

**快速看 RSS 峰值与当前：**
```bash
awk '/VmHWM|VmRSS|VmPeak|VmSize/{print}' /proc/1234/status
```

**查内存泄漏到具体库/区段：**
```bash
awk '/^[0-9a-f]+-/{print $1, $NF} {if(/Rss|Private_Dirty/)print}' /proc/1234/smaps
```
定位哪个映射（如 [heap]、某个 .so、anon 区段）在涨。

**PSS 汇总（最真实的"占用"）：**
```bash
awk '/^Pss/{p+=$2} END{print p" KB"}' /proc/1234/smaps
```

### 4. IO 分析

| 文件 | 内容 |
|------|------|
| `io` | rchar/wchar(读写字节) / syscr/syscw(系统调用次数) / read_bytes/write_bytes(真实落盘) / cancelled_write_bytes |
| `fd/` | 每个打开的文件描述符（符号链接） |

**判断进程是否在做大量 IO：**
```bash
awk '/^read_bytes|^write_bytes|^rchar|^wchar/{print}' /proc/1234/io
```
- rchar/wchar 大但 read_bytes 小 → 命中页缓存（不算真 IO）。
- read_bytes/write_bytes 大 → 真实磁盘压力。

**看进程打开了哪些文件/socket：**
```bash
ls -l /proc/1234/fd/         ;# 链接目标
readlink /proc/1234/fd/3
```
排查"文件句柄泄漏"：`ls /proc/1234/fd | wc -l` 对比 `ulimit -n`。

### 5. 线程/等待分析

```bash
ls /proc/1234/task | wc -l            ;# 线程数
awk '/^Threads/{print $2}' /proc/1234/status
```
- 某线程长期 D 状态 → 看 `/proc/1234/task/<tid>/stack` 和 `/proc/1234/task/<tid>/wchan`。
- `wchan`：内核中等待的函数名，如 `futex_wait_queue_me`、`io_schedule`、`poll_schedule_timeout`。

```bash
cat /proc/1234/task/<tid>/wchan    ;# futex_wait / io_schedule / ...
```
快速判断阻塞原因类型（锁/IO/睡眠）。

### 6. 上下文切换

```bash
awk '/voluntary_ctxt_switches|nonvoluntary_ctxt_switches/{print}' /proc/1234/status
```
- voluntary 多 → 主动让出（IO/锁/sleep）。
- nonvoluntary 多 → 被抢占，可能 CPU 被抢或优先级问题。

### 7. 信号 / 限制

```bash
cat /proc/1234/limits      ;# 各类 ulimit（文件数、栈大小、CPU 时间等）
cat /proc/1234/status | grep -i sig   ;# SigQ 当前/最大队列，SigPnd/SigBlk...
```

## 三、系统级分析（/proc 根目录）

### CPU

```bash
cat /proc/loadavg          ;# 1/5/15分钟负载 + 运行/总数 + 最近pid
cat /proc/stat             ;# 各CPU的user/nice/system/idle/iowait/irq...
```
**计算单核 iowait 占比（两次采样）：**
```bash
awk '/^cpu0 /{print "iowait="$6" total="$2+$3+$4+$5+$6+$7+$8}' /proc/stat
```
iowait 高 → IO 瓶颈（结合进程 io 文件定位元凶）。

### 内存

```bash
cat /proc/meminfo          ;# MemTotal/MemFree/MemAvailable/Cached/Buffers/SwapCached/...
```
- `MemAvailable` 比较准的可用量（含可回收缓存）。
- `Shmem`/`Slab` 异常大可能是 tmpfs 或内核 slab 泄漏。
- `SwapFree` 减少且 `pswpin/pswpout` 高 → 内存压力。

### 磁盘/块 IO

```bash
cat /proc/diskstats        ;# 每设备读写完成数、扇区、队列时间...
```
字段含义见 `Documentation/iostats.txt`。关键字段：
- 第 4 列：读完成次数；第 8 列：写完成次数。
- 第 7/11 列：读/写扇区数（×512 = 字节）。
- 第 13 列：IO 累计耗时（ms），除以采样间隔 = 平均队列占用。

**简捷版：** 一般直接用 `iostat -x 1`（基于 diskstats）。

### 网络

```bash
cat /proc/net/dev          ;# 每网卡 收发字节/包/错误/丢包
cat /proc/net/sockstat     ;# TCP/UDP/RAW socket 数
cat /proc/net/tcp          ;# TCP 连接表（状态、本地/远端地址十六进制）
cat /proc/net/snmp          ;# TCP/UDP/ICMP 统计（重传、RST、丢包）
cat /proc/net/netstat       ;# 扩展统计（TcpExt: 各种内核事件计数）
```
- TCP 重传：`/proc/net/snmp` 里 `RetransSegs`。
- listen drop/overflow：`/proc/net/netstat` 的 `ListenDrops`、`ListenOverflows`。
- `ss`/`netstat` 都是这些文件的封装。

### 中断 / 软中断

```bash
cat /proc/interrupts        ;# 每CPU每中断号计数
cat /proc/softirqs          ;# 软中断计数（NET_RX/TIMER/RCU/...）
cat /proc/irq/<n>/smp_affinity  ;# 中断绑核
```
网卡中断集中在单核 → 调整 smp_affinity 分散。

### 其他

```bash
cat /proc/pressure/{cpu,io,memory}   ;# PSI（压力失速信息），some/full 比例
cat /proc/vmstat                       ;# paging/swap/alloc 事件计数
cat /proc/schedstat                    ;# 调度器统计
cat /proc/sys/...                      ;# 可调内核参数
```

## 四、实战套路

### A. 找 CPU 飙高根因

```bash
# 1. 找占 CPU 高的进程/线程
top -H -p 1234            # 或
ps -L -o tid,pcpu,stat,comm -p 1234

# 2. 看该线程在内核做什么
TID=...
cat /proc/1234/task/$TID/stack
cat /proc/1234/task/$TID/wchan

# 3. 用户态栈（/proc 没有，用 perf）
perf top -p 1234
# 或
gstack 1234 | less
```

### B. 找内存泄漏

```bash
# 1. 观察 RSS 趋势
while true; do
  awk '/VmRSS|VmHWM/{print strftime("%T"), $0}' /proc/1234/status
  sleep 5
done

# 2. 定位增长区段
awk '/^[0-9a-f]+-/{reg=$1} /^Rss:/{print reg, $2}' /proc/1234/smaps | sort -k2 -n

# 3. 结合 smaps PSS 看是私有还是共享
grep -E 'Private_Dirty|Private_Clean' /proc/1234/smaps
```

### C. 找 IO 瓶颈元凶

```bash
# 1. 系统级
iostat -x 1                # %util、await 高的盘
cat /proc/pressure/io      # 看 IO 压力

# 2. 进程级：循环找 read_bytes/write_bytes 涨最快的
for p in /proc/[0-9]*; do
  awk -v p=${p##*/} '/^read_bytes|^write_bytes/{print p, $0}' $p/io
done

# 3. 看它在读写哪些文件
ls -l /proc/1234/fd | grep -v 'socket\|pipe\|anon_inode'
```

### D. 判断进程为什么卡（D 状态）

```bash
awk '/^State/{print $2}' /proc/1234/status     # D?
cat /proc/1234/stack                            # 内核栈看是 io_schedule/futex/...
cat /proc/1234/wchan
# 用户态：gstack / perf
```

## 五、自己写监控脚本示例

**每秒采集某进程 CPU% 和 RSS：**
```bash
#!/bin/bash
PID=$1
HZ=100
prev=$(awk '{print $14+$15}' /proc/$PID/stat)
while sleep 1; do
  cur=$(awk '{print $14+$15}' /proc/$PID/stat)
  rss=$(awk '/VmRSS/{print $2}' /proc/$PID/status)
  cpu=$(( (cur-prev)/HZ*100 ))   # 单核% （多核可能>100）
  echo "$(date +%T) cpu=${cpu}% rss=${rss}KB"
  prev=$cur
done
```

## 六、/proc 的局限与替代

| 需求 | /proc 不足 | 用什么 |
|------|-----------|--------|
| 用户态函数调用栈 | 不提供 | `perf record -g`、`gstack`、`py-spy`(Python) |
| 火焰图 | 无 | `perf + FlameGraph` |
| 系统调用追踪 | 无 | `strace -c -p PID` |
| 动态插桩 | 无 | `bpftrace`/`bcc`（eBPF） |
| 历史/趋势 | 只实时快照 | `sar`/`atop`/`Prometheus node_exporter` |
| 离线分析 | 无 | `perf` + `perf-archive` |

**eBPF 时代推荐组合：**
- 即时定位：`/proc` + `perf top` + `cat stack`
- 深度分析：`perf record -g` → 火焰图
- 动态追踪：`bpftrace`（锁、IO、延迟分布）
- 持续监控：`node_exporter`(读/proc) + Prometheus

---

## QEMU `-perfmap` 标志

QEMU 用 TCG 把客户机代码翻译（JIT）成宿主机代码运行。`perf` 默认只能看到宿主机地址，翻译出来的代码块没有符号，火焰图里会显示为 `[unknown]` 或一大片匿名地址。`-perfmap` 让 QEMU 边翻译边写出一个映射文件，`perf` 就能把这些 JIT 代码块解析回客户机的符号/地址。

下面用一个真实可复现的小例子对比。QEMU 11、x86-64 user 模式，profiling 一个静态编译的 guest 程序 `hot`，里面有一个故意写得很热的函数 `busy_calc`（被 `worker_a`/`worker_b` 调用）。vaScript
## 实验环境

```bash
# guest 程序：busy_calc 是热点
gcc -O2 -static -o hot hot.c
```
两轮采样（相同的 `-F 999 -g`，perf_event_paranoid=2，普通用户即可）：
- A：`qemu-x86_64 ./hot`（不加 perfmap）
- B：`qemu-x86_64 -perfmap ./hot`（加 perfmap）

## A. 不加 `-perfmap`

`perf report`（self 开销，节选）：
```
  Overhead  Command      Shared Object            Symbol
  18.47%   qemu-x86_64  [JIT] tid 3575306        [.] 0x00007fccb4064413
  12.32%   qemu-x86_64  [JIT] tid 3575306        [.] 0x00007fccb406444f
   9.32%   qemu-x86_64  [JIT] tid 3575306        [.] 0x00007fccb406441a
   9.17%   qemu-x86_64  [JIT] tid 3575306        [.] 0x00007fccb4064400
   8.18%   qemu-x86_64  [JIT] tid 3575306        [.] 0x00007fccb4064456
   ...
   0.43%   qemu-x86_64  qemu-x86_64              [.] helper_lookup_tb_ptr
```
调用图里还混着一堆 `[unknown] 0x000000000000010e ...`（无法解析的帧）。
**看到的全是 `[JIT]` + 匿名宿主地址**，根本看不出是 `busy_calc` 还是 `main`，更分不清是哪个 guest 函数。

## B. 加 `-perfmap`

QEMU 同时写出 `/tmp/perf-<pid>.map`，节选与热点相关的几行：
```
7fd6ac064240 f  busy_calc
7fd6ac06424f 8  busy_calc+0x27
7fd6ac064257 1e busy_calc+0x2a
7fd6ac064275 12 busy_calc+0x30
7fd6ac064287 1d busy_calc+0x37
7fd6ac0642a4 57 busy_calc+0x39
```
格式：`<宿主地址> <大小> <guest符号+偏移>`。perf report 时按 pid 自动加载这个文件，得到：
```
  Overhead  Command      Shared Object            Symbol
  28.22%   qemu-x86_64  [JIT] tid 3584557        [.] busy_calc+0x2a
  19.08%   qemu-x86_64  [JIT] tid 3584557        [.] busy_calc+0x37
  15.36%   qemu-x86_64  [JIT] tid 3584557        [.] busy_calc+0x20
  14.33%   qemu-x86_64  [JIT] tid 3584557        [.] busy_calc+0x27
  11.55%   qemu-x86_64  [JIT] tid 3584557        [.] busy_calc+0x30
   4.32%   qemu-x86_64  [JIT] tid 3584557        [.] busy_calc+0x39
   0.61%   qemu-x86_64  qemu-x86_64              [.] helper_lookup_tb_ptr
```
调用图也变成可读的 guest 调用链：
```
--- _start → main → (cpu_loop → cpu_exec → tb_gen_code 等是 QEMU 自身，下同)
```
一眼看出：**热点全在 guest 的 `busy_calc`**，且按指令偏移细分（`+0x2a` 是循环里那条 LCG 乘法指令）。

## 关键差异一览

| 维度 | 不加 `-perfmap` | 加 `-perfmap` |
|---|---|---|
| Symbol 列 | `[.] 0x00007fccb4064413` 等宿主匿名地址 | `[.] busy_calc+0x2a` 等 guest 符号 |
| Shared Object | `[JIT] tid ...`（无信息） | `[JIT] tid ...`（但符号已解析） |
| 调用图 | 大量 `[unknown]` 帧，链断在 JIT 代码 | 能看到 guest 函数名与偏移 |
| 能否归因到 guest 代码 | 不能 | 能，且到指令级 |
| 文件副作用 | 无 | 多一个 `/tmp/perf-<pid>.map`（本例 6567 行） |
| 采样本身 | 9165 samples | 8015 samples（量级一致， perfmap 几乎不增加运行开销） |

## 还想看到更准：`-jitdump`

`-jitdump` + `perf inject --jit` 在此基础上还能带源文件/行号，火焰图里就能直接定位到 hot.c 的某一行。本例用 `-perfmap` 已经足够说明「有没有这个开关，perf 看到的是匿名地址 vs 真实函数名」这件事。

## 复现命令（这台机器上实测可用）

```bash
mkdir -p ~/perfmap-demo && cd ~/perfmap-demo
# hot.c 内容见上文
gcc -O2 -static -o hot hot.c

# A: 不加
perf record -F 999 -g -o perf-nomap.data -- qemu-x86_64 ./hot
perf report -i perf-nomap.data --stdio --no-children -g none | head -30

# B: 加 perfmap
perf record -F 999 -g -o perf-map.data -- qemu-x86_64 -perfmap ./hot
perf report -i perf-map.data --stdio --no-children -g none | head -30
# 顺便看映射文件
ls -t /tmp/perf-*.map | head -1 | xargs head
```
> 小提示：若想调用图里少混入 QEMU 翻译调度开销、纯看 guest，别加 `-d nochain`——本例实测 nochain 反而把 90% 样本塞进了 QEMU 的 `tb_gen_code/perf_report_code` 派发路径，掩盖了 guest 热点。perfmap 默认（有 TB 链接）才是最贴近真实热点的视图。
## 1. 它做什么

```
-perfmap        generate a /tmp/perf-${pid}.map file for perf
-jitdump        generate a jit-${pid}.dump file for perf
```

- **`-perfmap`**：在 `/tmp/perf-<qemu_pid>.map` 追加文本行，格式（与 perf 的 JIT 接口一致）：
  ```
  <host_addr_hex> <size_hex> <symbol_name>
  ```
  例如 `0x7f8a3c001000 0x40 guest_0x401000:main+0x10`
- 文件随翻译实时增长，`perf record/report` 读取它来把宿主 PC 翻译成客户机符号。
- system 模式和 user 模式（`qemu-x86_64 -perfmap`）都支持。

## 2. 与 `-jitdump` 的区别

| | `-perfmap` | `-jitdump` |
|---|---|---|
| 文件 | `/tmp/perf-<pid>.map` 文本 | `jit-<pid>.dump` 二进制 |
| 解析 | perf 直接读 | 需 `perf inject --jit` 后处理 |
| 信息 | addr/size/符号 | 还可带源文件行号、inline 信息 |
| 用法 | 简单 | 更精确，能出带行号的火焰图 |

两者可同时开启，但一般按需选其一。

## 3. 典型用法（profiling 客户机代码）

```bash
# 1) 启动 QEMU，开启 perfmap。建议关掉 TB 链接以获得更准的调用图
qemu-system-x86_64 \
    -perfmap \
    -accel tcg,thread=multi \
    -d nochain \
    ...你的其它参数...

# 或 user 模式
qemu-x86_64 -perfmap -d nochain ./guest_app
```

`-d nochain`（或 `-accel tcg,tb-size=...` 调小）可选，但建议加：TB 链接会把多个块串起来，导致 perf 看到的 PC 与映射边界不一致，影响符号解析精度。代价是性能略降。

```bash
# 2) 同时用 perf 采样 QEMU 进程（宿主机上）
sudo perf record -F 999 -g -p <qemu_pid> -- sleep 30
# 或对 user 模式整条命令：
perf record -F 999 -g -- qemu-x86_64 -perfmap ./guest_app
```

```bash
# 3) 查看
sudo perf report -i perf.data        # 自动用 /tmp/perf-<pid>.map 解析
# 或火焰图：
perf script -i perf.data | ./stackcollapse-perf.pl | ./flamegraph.pl > fg.svg
```

## 4. 注意事项

1. **map 文件与 perf.data 要同机器、同一次运行**：perf 在 report 时按 QEMU 的 pid 去找 `/tmp/perf-<pid>.map`，所以别清掉它，且需要在 report 时还存在。
2. **权限**：`perf` 通常要 root；map 文件由 QEMU（普通用户）写，路径固定 `/tmp`。
3. **system 模式符号**：QEMU 需要能解析客户机内核/用户态符号。system 模式下 `-perfmap` 输出的符号主要基于翻译块的客户机虚拟地址；要想得到函数名，可配合 guest 自带的符号表（如 `vmlinux` + `-d nochain`）或用插件 `qemu_plugin_insn_symbol()`。
4. **TB 缓存被冲刷**：代码 cache flush 后会重新翻译，map 文件会追加新条目（地址可能变），这是正常的——perf 按时间戳匹配，旧条目仍有效。
5. **多线程 MTTCG**：每个 vCPU 线程都向同一个 map 文件写，内部加锁；地址空间独立，符号仍能正确归属。
6. **不要混淆**：`-perfmap` 解析的是「QEMU 翻译出来的宿主代码块 → 客户机 PC/符号」，不是给宿主机 perf 加 QEMU 自身符号（QEMU 自身用 `-g` 编译 + 带 debug 符号即可）。

## 5. 一行最小示例

```bash
sudo perf record -F 999 -g -- \
    qemu-x86_64 -perfmap -d nochain /path/to/guest-binary
sudo perf report -i perf.data
```

如果你需要 system 模式下出**带行号的火焰图**，推荐用 `-jitdump` + `perf inject --jit`：

```bash
qemu-system-x86_64 -jitdump ...
sudo perf record -F 999 -g -p <pid> -- sleep 30
sudo perf inject --jit -i perf.data -o perf.data.jit
sudo perf report -i perf.data.jit
```

需要具体的某场景（profiling 客户机内核、profiling 客户机用户态程序、ARM/RISC-V target）的完整命令链，告诉我即可。# qemu perfmap 标志 
# 分析qemus 是否卡死

QEMU 看活动核心 / 内存等运行数据，主要走 **HMP（human monitor）/ QMP**。下面按"如何进入、看什么、命令清单"来讲。

## 一、连接 monitor

启动时预留 monitor：
```bash
# 直接终端（最简单，调试用）
qemu-system-x86_64 ... -monitor stdio

# 或 socket，便于脚本/hmp/qmp
qemu-system-x86_64 ... \
    -monitor unix:/tmp/qmp-sock,server,on=off
# 客户端：
socat - UNIX-CONNECT:/tmp/qmp-sock        # HMP
# QMP 形式：
socat - UNIX-CONNECT:/tmp/qmp-sock        # 首条 qmp_capabilities
```

若 QEMU 已经在跑、没留 monitor，可用 `guest-agent`（`-chardev ... -device virtserialport` + `qemu-ga` in guest）或直接在 guest 内自己采集（`/proc` 等）。

## 二、查看活动核 / 负载（HMP 命令）

在 monitor 里：
```
(qemu) info cpus
```
输出示例：
```
* CPU #0: thread_id=12345 (vCPU running)
  CPU #1: thread_id=12346 (vCPU running)
  CPU #2: thread_id=12347 (vCPU halted)
  ...
```
- `running` = 该 vCPU 当前在执行 guest 指令；`halted` = 已 halt，等待中断（即活动为 0）。
- "活动核数"就是 `running` 的计数。

```
(qemu) info status
```
显示 VM 整体状态：`running` / `paused` / `shutdown` 等。

```
(qemu) info registers
```
看各 vCPU 寄存器（含 PC/RIP），间接判断在跑什么。

## 三、查看内存

```
(qemu) info memory
```
Some versions don't provide this; alternative:
```
(qemu) info mtree            # 整个地址空间布局（flat view），看 RAM/ROM/MMIO 区段
(qemu) info mtree -f         # flat view，最直观
(qemu) info mtree -e         # 仅当前物理地址空间
(qemu) info qtree             # 设备树里也能看每个 pc-dimm 等内存设备
```

宿主机侧 RAM 占用OfClass：
```bash
ps -o rss,vsz,comm -p <qemu_pid>
awk '/VmRSS|VmHWM|VmSize|VmPeak/{print}' /proc/<qemu_pid>/status
```
QEMU 默认不预分配，除非加 `-object memory-backend-ram,size=4G,prealloc=on` 或 `-mem-prealloc`。所以 RSS 会随 guest 写入增长。

## 四、QMP 等价命令（适合脚本化）

先发：
```json
{"execute":"qmp_capabilities"}
```
然后：
```json
{"execute":"query-cpus-fast"}      // 看每个 vCPU 的 thread_id / halted 状态
{"execute":"query-status"}          // VM 状态
{"execute":"query-memory-size"}     // 当前内存（部分版本）
{"execute":"query-memory-devices"}  // DIMM/pc-dimm 列表与大小
{"execute":"query-balloon"}         // 若启用 ballooning，看当前实际大小
```
其中 `query-cpus-fast` 比 `query-cpus` 快，不阻塞 vCPU，推荐用。

各字段含义：
- `cpu-index` / `thread-id` / `qom-path`：标识
- `halted`: true 表示该 vCPU 不在跑（"非活动核"）
- `props`：Socket/Core/Thread 拓扑（对应 `-smp` 的 `sockets=,cores=,threads=`）

## 五、HMP 速查表（最常用）

```
info cpus                # vCPU 状态：running / halted → 活动/非活动
info status              # VM running/paused
info registers           # 各 vCPU 寄存器
info mtree [-f] [-e]     # 地址空间/内存布局
info memory              # （若可用）内存摘要
info qtree                # 设备树（内存设备、CPU 等）
info balloon             # ballooning 后的当前内存大小
info numa                # NUMA 节点状态（若配了 -numa）
info pci
info history
```

## 六、外部更直观的工具

宿主机层面看 QEMU 的每 vCPU 对应线程：
```bash
top -H -p <qemu_pid>         # 每线程 CPU%，活动 vCPU 对应的 Thread 高，halted 的接近 0
ps -L -o tid,pcpu,stat,comm -p <qemu_pid>
```
活动 vCPU 数 ≈ 上面的 `R`/`running` 线程数。memory 看 RSS 即可。

guest 内部（若只要 guest 视角的活动核/内存）：
```
guest$ nproc; free -h; cat /proc/loadavg; cat /proc/stat
```
若想用 `qemu-ga` 在 host 不登录 guest 也能拿到，配好 `virtio-serial` + `qemu-guest-agent` 后可通过 QMP `guest-get-cpu`、`guest-get-memory`等。需要这一块完整命令链可以告诉我。

# 如何使用上述工具对我们的内核进行开发
