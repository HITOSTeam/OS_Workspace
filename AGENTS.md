## 用户补充需求

<!-- 请在此处补充本项目的协作、代码风格和验收要求。以下规则均应同时遵守。 -->

## 语言与改动范围

- 新增或修改的代码注释一律使用中文；保留第三方代码和未修改的既有注释原样不动。
- 不覆盖、不回退用户已有改动；先查看工作树状态，再修改重叠文件。
- 上板过程中不写入 SPI、eMMC 或 SD 卡，除非用户明确授权，并在操作前确认准确设备路径。
- 不声称真板已验证成功；必须以完整串口日志作为证据。

## 构建特性与 QEMU 兼容性

- `ARCH=riscv64` 表示 RISC-V 架构，不表示具体板卡；真板必须显式设置 `BOARD=visionfive2`。
- `BOARD=visionfive2` 会启用 Cargo feature `riscv-board`，其内部依赖 `visionfive2`。所有仅属于真板的代码、地址和初始化逻辑必须用 `#[cfg(feature = "visionfive2")]` 或 `#[cfg(not(feature = "visionfive2"))]` 隔离。
- 默认 QEMU 路径必须保持行为不变：不把真板 MMIO 地址、JH7110 中断控制器假设或 SDIO 驱动放入无条件代码。
- 每次修改真板相关代码后，至少验证以下两种构建，并最后构建真板版本，避免 QEMU ELF 覆盖 TFTP 发布目录中的 `os`：
  ```bash
  cd /workspaces/OS_Workspace_xcy/OS_Workspace/os
  make elf ARCH=riscv64 BOARD=qemu MODE=release
  make elf ARCH=riscv64 BOARD=visionfive2 MODE=release
  ```
- 修改代码尽可能保证代码简洁,使用最少的代码来实现
- 代码中尽量不要硬编码寄存器地址,相关的地址尽量从DTB中读取,相关的读取代码可以参考linux的原代码,linux原始代码位于`/workspaces/OS_Workspace_xcy/linux-7.1.8`,尽量使用codegraph查看
- `/workspaces/OS_Workspace_xcy/StarryOS`里面是一个简单的rust内核,他有和这个板子的适配代码,我们也是可以参考的

## VisionFive 2 上板约定

- 板型：StarFive VisionFive 2，原厂 U-Boot 2021.10，串口为 3.3V TTL、115200、8N1。
- 串口接线：板子 Pin 8 TX -> USB-TTL RX，Pin 10 RX -> USB-TTL TX，Pin 9 GND -> GND；USB-TTL 不连接供电脚。
- 物理网口为主机 `enp1s0`，地址为 `10.42.0.1/24`；不要误用用于外网的 `enp2s0`。
- 只有一个进程可交互使用串口。用户使用 picocom 时，必须先按 `Ctrl-A`、`Ctrl-X` 退出，代理才可接管。
- 上电、断电、插拔 SD 卡和按键由用户执行；代理应先启动串口日志与 TFTP，再明确请求用户操作。

## U-Boot 与 TFTP 启动

- 原厂 U-Boot 的 `bootelf` 仅支持 `bootelf [-p|-s] [address]`，不支持 `-d`，且传递 C ABI 的 `argc/argv`，不能直接满足内核要求的 RISC-V `(hart_id, dtb_pa)` 启动 ABI。
- 真板应使用 `booti` 交接控制权。普通裸二进制不含 Linux RISC-V Image 头，需配合 `tools/vf2_booti_trampoline.S` 生成的 4 KiB 跳板。
- 每次真板构建后生成发布文件：

  ```bash
  cd /workspaces/OS_Workspace_xcy/OS_Workspace/os
  llvm-objcopy -O binary --strip-all \
    ../target/riscv64gc-unknown-none-elf/release/os \
    ../target/riscv64gc-unknown-none-elf/release/os.bin
  riscv64-unknown-elf-as -march=rv64gc -mabi=lp64d \
    -o ../target/riscv64gc-unknown-none-elf/release/vf2_booti_trampoline.o \
    tools/vf2_booti_trampoline.S
  llvm-objcopy -O binary --strip-all \
    ../target/riscv64gc-unknown-none-elf/release/vf2_booti_trampoline.o \
    ../target/riscv64gc-unknown-none-elf/release/vf2_booti.img
  ```

- TFTP 根目录为 `target/riscv64gc-unknown-none-elf/release`，发布 `os.bin` 与 `vf2_booti.img`。U-Boot 临时环境（不执行 `saveenv`）：

  ```text
  setenv ipaddr 10.42.0.100
  setenv serverip 10.42.0.1
  setenv ethact ethernet@16030000
  ping ${serverip}
  tftpboot 0x80200000 os.bin
  tftpboot 0xc0000000 vf2_booti.img
  booti 0xc0000000 - ${fdtcontroladdr}
  ```

- 真实内核必须先加载到 `0x80200000`。不要把含 ELF 头的 `os` 也下载到该地址后用 `bootelf -p`，否则 U-Boot 的装载源与段目标重叠；跳板从 `0xc0000000` 重定位到 `0xb0000000` 后跳入内核。

## 调试与验收顺序

1. 先确认 U-Boot、`ping`、TFTP 文件名和传输字节数。
2. 确认内核日志至少经过 DTB、SBI、内存初始化；若停止，记录最后一条日志并只修改对应阶段。
3. 接着验证 `[vf2-sd]`、GPT p1/p2、官方 rootfs 选择以及 `/user` 程序发现。
4. 最后运行 CAgent 和 BuildStorm。每次失败应保存从启动命令起的完整串口输出。
