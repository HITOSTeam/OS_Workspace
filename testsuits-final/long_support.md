`dbar` 是 **LoongArch** 架构的数据屏障指令，其完整格式为：

## 指令格式

```
dbar hint
```

其中 `hint` 为 **9 位立即数**（0–511），用于指定屏障的"暗示"类型。未使用的 hint 值应被视为**完全屏障**（保守处理）。

## 各 hint 值含义

| hint 值 | 含义 | 说明 |
|---------|------|------|
| `dbar 0` | 完全屏障 | 等价于 `sync`/`dmb sy`，所有之前的访存完成后才继续。最常用。 |
| `dbar 1` | Store-Store 屏障 | 保证该指令之前的所有 **store** 在之后的 store 之前完成。等价 ARM `dmb ishst`。 |
| `dbar 2` | Store-Load 屏障 | 之前的 store 在之后的 load 之前完成。 |
| `dbar 3` | Load-Store 屏障 | 之前的 load 在之后的 store 之前完成。 |
| `dbar 4` | Load-Load 屏障 | 保证该指令之前的所有 **load** 在之后的 load 之前完成。 |

> 5–511：当前规范中保留/未定义，实现应保守地当作完全屏障处理。

## 典型用法

```asm
# 完全屏障（最常见）
dbar 0

# 自旋锁释放前，保证 store 顺序
dbar 1

# 多核间通过共享内存通信，保证对端读到的数据顺序
dbar 0
```

## 作用

- 约束 **load/store 的执行顺序**与 **可见性**，防止 CPU 或硬件乱序。
- 不影响寄存器值，仅影响内存访问的时序。

## 对比其他架构

| LoongArch | ARM64 | x86 | RISC-V |
|-----------|-------|-----|--------|
| `dbar 0` | `dmb sy` / `dmb ish` | `mfence` | `fence rw,rw` |
| `dbar 1` | `dmb ishst` | — (x86 默认 store-store 有序) | `fence w,w` |
| `dbar 4` | `dmb ishld` | `lfence` | `fence r,r` |

> 注：LoongArch 还有 `ibar hint`（指令屏障），用于保证指令缓存与数据一致，常见为 `ibar 0`。
