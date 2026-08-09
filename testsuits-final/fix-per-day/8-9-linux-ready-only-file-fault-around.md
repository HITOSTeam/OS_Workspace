# 8-9 ready-only file fault-around 降低 BuildStorm 缺页次数

## 问题概述

工具链、动态库和 rlib 的物理文件页已经能由 inode page cache 在多个进程间共享，但每个
新 rustc/linker 地址空间仍逐 4 KiB 触发硬件 page fault。每个热页 fault 都重复执行：

```text
trap -> VMA/MapArea 查找 -> 页表 walk -> inode page tree 查找
     -> data_frames BTree 插入 -> 返回用户态
```

物理页是热的，所以 shared buddy 几乎不工作；但 fault/trap、mm 锁、页表 walk 和每-mm
BTree 元数据仍按页支付。BuildStorm 的新进程密度把这条 CPU 路径放大成主要瓶颈。

## 如何发现

### 1. perf 把热点定位到逐页 fault 元数据

带 `-perfmap` 的 RISC-V BuildStorm profile 中出现精确专门化：

```text
BTreeMap<VirtPageNum, FrameTracker>
VacantEntry::insert
insert_recursing
split
```

全 `os/src` 只有 `MapArea::data_frames` 使用该 K/V 类型；同时 profile 中
`MemorySet::fault`、slab、页表和 QEMU `helper_lookup_tb_ptr` 都较热。shared high-order
buddy 占比很低，说明“热文件页仍逐页建立新 mm 元数据”比继续调 buddy 更符合现场。

诊断资产：

```text
testsuits-final/.tmp/final-runs/20260809-riscv-shared-highorder-perfmap-full-226/
```

`-perfmap` 用于归因，不用于正式耗时。完整 production 测试关闭了 perfmap 和
`DEBUG_PERF`。

### 2. 精确 1 页消融闭合因果链

在同一份代码上只把 fault-around 窗口从 16 页改为 1 页，其余配置不变。聚焦 workload
末次 `/proc/perf`：

| 指标 | 1 页基线 | 16 页候选 | 变化 |
| --- | ---: | ---: | ---: |
| `file_fault_events` | 52,831 | 26,028 | **-50.73%** |
| page-cache misses | 6,592 | 6,592 | 相同 |
| `file_fault_ptes_mapped` | 52,817 | 53,608 | +1.50% |
| `mm_data_frame_inserts` | 77,982 | 79,047 | +1.37% |
| heap allocation failures | 0 | 0 | 相同 |

候选没有减少最终需要的 PTE 或 `data_frames` 节点，也没有改变冷页 I/O；它把相近数量的
映射工作合并到一半的硬件 fault 中。14,357 次 fault-around 尝试扫描 226,778 页，找到
97,993 个 Ready 页，平均每次约 6.82 页可直接复用。

原始日志：

```text
.tmp/fault-around/baseline-1page-focus-1/serial.log
.tmp/fault-around/candidate-focus-1/serial.log
```

## Linux 对照

本地 Linux 参考树为 `exampleOs/linux` commit
`4549871118cf616eecdd2d939f78e3b9e1dddc48`。

- `mm/memory.c::fault_around_pages` 默认 64 KiB，即 16 个 4-KiB 页；
- `do_fault_around()` 把窗口自然对齐，并限制在同一 VMA 和末级 PTE table；
- `do_read_fault()` 先调用 VMA 的 `map_pages()`，当前页不 Ready 时才回退普通 fault；
- `mm/filemap.c::filemap_map_pages()` 只遍历 uptodate、可映射的 folio，跳过 locked/未就绪
  页，并按当前 `i_size` 限制 EOF；
- COW/write fault 不走这条只读 fault-around 快路。

CongCore 没有 folio/XArray/RCU fault path，本批复用 inode-local Ready page tree 和现有
mm 锁，保持相同的可观察语义与有界窗口。

## 怎么解决

### 1. 页缓存返回 inode-local mapping handle

`file_page_cache_get_or_load()` 现在返回当前 `FrameTracker`、cache hit/miss 和其所属
`FilePageCacheMapping`。fault 不必再次查全局 `(dev, ino)` BTree；它在 inode page-tree
锁下扫描范围内的 `Ready` slot，遇到 `Loading` 或缺页直接跳过，不等待也不启动额外 I/O。

### 2. 16 页固定栈 batch

fault-around batch 使用固定 `[Option<_>; 16]`，没有每-fault Vec 分配。窗口：

- 自然按 16 页/64 KiB 对齐；
- 不跨当前 `VmRegion`、对应 `MapArea`、SIGBUS/EOF 边界；
- 不跨 512-entry 末级页表；
- 编译期断言页数为 2 的幂且不大于末级页表容量。

当前 fault 页始终排第一。它用 fallible `try_map_cached()` 先建立必要页表；只有成功后才
用同一个 `PageWalkCache` 安装邻页，避免 OOM 时只留下 speculative mapping。

### 3. 保留文件映射语义

- 只对 regular-file read/exec fault 批量映射；write/private-COW 仍单页处理；
- 提交前重新校验 VMA、MapArea 起止、权限和 PTE；已有有效 PTE 直接跳过；
- executable 页逐 frame 复用既有 I-cache publication；
- MAP_SHARED 每页更新 resident backing ref；
- 初版每个新 PTE 调用一次架构 hook；后续提交
  `9cbde1a48ae50693be0775271beac5722c04673e` 已改为公共 range publication，
  一批 PTE store 完成后只调用一次架构 range hook；
- truncate/EOF、madvise、COW 和 reverse-mm 失效机制未绕过。

### 4. 增加最小因果计数

`DEBUG_PERF=true` 时 `/proc/perf` 新增 file fault hit/miss、窗口、Ready 页、实际安装 PTE
和 `data_frames` insert 计数。正式源码已恢复 `DEBUG_PERF=false`。

## 对应提交

| 项目 | 值 |
| --- | --- |
| `os/` 基线 | `89458995a7a504598b960885548b95b8b2bcef1c` |
| `os/` 修复 | `bd250cd879cb13ba6afe9ff3d12b1ee26573f2ef` |
| 提交标题 | `mm: map ready file pages around faults` |
| 顶层集成 | 本说明文档所在提交 |

## 对因提升

### 精确 B-C-C-B exec/file-page-cache A/B

QEMU 11.0.3、RISC-V64、8 hart、8 GiB、`rv64,svvptc=true`，每次用相同 raw backing
创建独立 qcow2 overlay。顺序为 baseline -> candidate -> candidate -> baseline；每次启动
取 7 个外层样本，每侧合并 14 个样本：

| 指标 | 1 页基线 | 16 页候选 | 改善 |
| --- | ---: | ---: | ---: |
| 样本数 | 14 | 14 | 相同 |
| 跨样本中位数 | 264,641.5 us | 223,206.0 us | **-15.66%** |
| 等价吞吐 | 1.000x | 1.186x | **+18.56%** |
| failures | 0 | 0 | 无回归 |

四次启动的批内中位数依次为：baseline 258,741 us、candidate 218,962 us、candidate
224,869 us、baseline 278,817 us，方向没有被运行顺序反转。

```text
.tmp/ablate/fault-around-base-b1/results.csv
.tmp/ablate/fault-around-candidate-c1/results.csv
.tmp/ablate/fault-around-candidate-c2/results.csv
.tmp/ablate/fault-around-base-b2/results.csv
```

### BuildStorm gate

关闭 perfmap、`DEBUG_PERF=false` 的独立 overlay gate 在 guest 约 361 秒时：

| 指标 | 1 页基线 | 16 页候选 | 改善 |
| --- | ---: | ---: | ---: |
| deps | 87 | 109 | **+25.3%** |
| output bytes | 3,173 | 3,790 | **+19.4%** |

候选约 236 秒已达到基线 361 秒的 deps≈87 里程碑，里程碑时间约缩短 34.6%。这是前半程
进度 gate，不替代正式计时。

```text
testsuits-final/.tmp/final-runs/20260809-riscv-fault-around-base-gate-229/
testsuits-final/.tmp/final-runs/20260809-riscv-fault-around-candidate-gate-230/
```

### 完整 RISC-V BuildStorm

production 内核关闭 perfmap 和 `DEBUG_PERF`，使用 8 hart/8 GiB 和独立 qcow2 overlay：

```text
BUILDSTORM_TOOLCHAIN ok
BUILDSTORM_MINIBUILD ok
BUILDSTORM_COMPILE mode=multi ok=true elapsed_s=1100.23 cores=8 bytes=1681000 arch=riscv64
```

| 指标 | 评分参考 | 本轮 | 变化 |
| --- | ---: | ---: | ---: |
| 正式编译时间 | 1,616.09 s | 1,100.23 s | **-31.92%** |
| 等价吞吐 | 1.000x | 1.469x | **+46.89%** |
| 脚本分 | — | **180/180** | 满分 |
| guest 完成 uptime | — | 3,791.41 s | rc=0 |

评分参考包含此前内核的整体差异，不能把完整 31.92% 全归因给本提交；本提交的独立因果
收益以 1 页/16 页 B-C-C-B 的 15.66% 延迟下降为准。

完整运行没有 panic/OOM，QEMU peak RSS 5,656,180 KiB、VmSwap 始终 0。正式日志：

```text
testsuits-final/.tmp/final-runs/20260809-riscv-fault-around-production-full-231/
```

最终抓取器曾因匹配到终端回显命令里的结束 marker 而生成假 0 分。原 overlay 保留后，
通过只读 child overlay 重抓 `/work/buildstorm-full.out`，官方 judge 得到 180/180。原始
`score.json` 保留作采集器假阴性证据，正确结果在 `score-recaptured.json`、
`judge-recaptured.log` 和 `recapture.log`；`recapture-notes.txt` 记录了恢复过程。

## 回归验证

RISC-V 聚焦运行态回归 7/7：

- `file_mmap_lazy_fault_smoke`；
- `private_file_page_cache_smoke`；
- `private_file_madvise_dontneed_smoke`；
- `shared_file_alias_smoke`；
- `shared_file_truncate_cache_smoke`；
- `riscv_icache_smp_smoke`；
- `exec_file_page_cache_perf_smoke`。

1 页消融与 16 页候选两侧均为 7/7。RISC-V、LoongArch64 softfloat `cargo check`、
RISC-V release build、`cargo fmt --all -- --check` 和 whitespace check 均通过，只有仓库
既有 warning。

## 当前边界与下一步

- fault-around 合并了 trap/mm walk，但每个页仍插入一个 `data_frames` BTree 节点；
- 下一项高杠杆应是 Linux 式固定 `PageDesc[]`/mapcount，逐步删除每-mm ownership shadow；
- 冷 cache miss 仍以 4 KiB I/O 为主，后续应让 mmap/exec 直接使用统一 address_space
  folio，并做 16--64 KiB 自适应 readahead；
- 不应只看 QEMU `helper_lookup_tb_ptr` 百分比；后续同时记录绝对 cycles、fault 数和最终
  elapsed，避免其他热点下降后占比上升造成误判。
