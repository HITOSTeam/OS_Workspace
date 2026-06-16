# CongCore Workspace

这是项目主工作区仓库。

当前仓库结构采用：

- 根仓库：维护主工作区内容，例如 `user/`、`vendor/`、`ext4-fs/`、`ext4-fs-packer/`、工具脚本和顶层配置
- `os/`：独立仓库，维护内核代码
- `OSGuide/`：独立仓库，维护设计文档和测试进度

## 快速开始

首次获取代码：

```sh
git clone <根仓库地址> CongCore
cd CongCore
git submodule update --init --recursive
```

查看当前工作区状态：

```sh
bash tools/status_all.sh
```

运行测试前先产生测试文件 从testsuits 来命名为 sdcard-rv.img 或者 sdcard-la.img 
-  os挂载两个img分别是 包含基础目录+user下自定义程序的 由ext4-packer产生的镜像 和 这里的测试镜像文件  
测试文件由 testsuits得来
- 请备份测试文件。某些测试可能会破坏镜像。
运行一次集成测试：

```sh
ARCH=riscv64 bash os/run.sh
```

## LoongArch 内核布局与 trap 入口

LoongArch QEMU `virt -m 1G` 的可用高端 RAM 通常位于
`0x8000_0000..0xb000_0000`。当 `os/src/linker_loongarch.ld` 中设置：

```ld
BASE_ADDRESS = 0x80000000;
```

并使用 ELF 方式启动内核：

```sh
qemu-system-loongarch64 -machine virt -kernel kernel_release.load.elf ...
```

QEMU 会按照 ELF program header 中的物理地址装载各个 `LOAD` 段。因此
`BASE_ADDRESS` 会决定内核入口和各段实际被放置的物理地址。若改用裸 `bin`
启动，则启动器需要手动把镜像放到同一个链接基址，并从该入口地址跳转。

以 `BASE_ADDRESS = 0x80000000` 为例，内核静态段大致布局如下，具体边界以
启动日志或 `readelf -l/-S` 为准：

| 区域 | 地址范围示例 | 权限/用途 |
| --- | --- | --- |
| `.text` | `0x8000_0000..` | 内核代码，恒等映射，`R | X` |
| `strampoline` | `0x8000_1000` 附近 | trap trampoline 代码页，属于 `.text.trampoline` |
| `.rodata` | `.text` 之后 | 只读数据，恒等映射，`R` |
| `.data` | `.rodata` 之后 | 可写数据，恒等映射，`R | W` |
| `.bss` | `.data` 之后 | 零初始化数据，包含内核栈和静态 `HEAP_SPACE`，恒等映射，`R | W` |
| `ekernel` | `.bss` 结束 | 内核静态镜像结束，frame allocator 从这里之后回收高端物理页 |

`strampoline` 本身是内核 `.text` 中的一页代码，链接脚本通过
`strampoline = .; KEEP(*(.text.trampoline));` 标记它的位置。内核页表会把这页
代码额外映射到固定虚拟地址 `TRAMPOLINE`；每个用户页表也会映射同一物理页。
这样从用户态发生 syscall、异常或中断时，即使当前使用的是用户页表，CPU 也能
跳到可执行的 trampoline 代码，再切换回内核页表并进入 Rust trap handler。

LoongArch trap 入口不是始终固定在同一个地址：

- 内核初始化时，`init_trap()` 将 `EENTRY` 设为 `alltraps_k`，用于处理内核态 trap。
- 准备返回用户态时，`trap_return()` 跳到 trampoline 中的 `restore` 代码。
- `restore` 会切到用户页表，并把 `EENTRY` 改为用户可见的 `TRAMPOLINE` 地址。
- 用户态 trap 进入 trampoline 的 `alltraps`，保存用户上下文、切回内核页表，
  然后调用内核 trap handler。
- 进入内核处理路径后，内核会再次把 `EENTRY` 保持为 `alltraps_k`，避免内核态
  trap 误走用户 trampoline 流程。

注意静态内核 heap 的大小会直接撑大 `.bss`。如果 `HEAP_SPACE` 过大导致
`.bss/ekernel` 超过 DTB 中可用 RAM 的结尾，`clear_bss()` 早期清零时就可能写到
无效物理地址并死机。调整 `KERNEL_HEAP_SIZE` 时，需要同时检查 ELF 段边界和
QEMU/DTB 中的 RAM 范围。

## 提交规则

- 修改 `os/`：在 `os/` 仓库提交
- 修改 `OSGuide/`：在 `OSGuide/` 仓库提交
- 修改其他内容：在根仓库提交

如果修改了 `os/` 或 `OSGuide/`，请在对应仓库提交后，再回到根仓库更新 submodule 指针。

## 进一步说明

- 协作流程见 [COLLABORATION.md](./COLLABORATION.md)
- （可选） 可配置exampleOs文件夹供agent参考，详情参考AGENTS.md。
