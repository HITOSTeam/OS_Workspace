# 提交仓库构建入口
#
# make all      同时生成双架构内核与本地用户程序盘
# make build-rv 只编译 RISC-V 内核 ELF
# make build-la 只编译 LoongArch 内核 ELF
# make disk-rv  只制作 RISC-V 本地磁盘
# make disk-la  只制作 LoongArch 本地磁盘

MODE          ?= release
FINAL_TEST    ?= 1
SMP           ?= 12
MEM           ?= 8G
QEMU_TIMEOUT  ?= 0

OS_DIR        := os
TARGET_DIR    := target
CARGO_ROOT_CONFIG := cargo-config/root-config.toml
RISC_TARGET   := riscv64gc-unknown-none-elf
LOONG_TARGET  := loongarch64-unknown-none
RISC_ELF      := $(TARGET_DIR)/$(RISC_TARGET)/$(MODE)/os
LOONG_ELF     := $(TARGET_DIR)/$(LOONG_TARGET)/$(MODE)/os
RISC_DISK_SRC := ext4-fs-packer/target/fs-riscv64.ext4
LOONG_DISK_SRC:= ext4-fs-packer/target/fs-loongarch64.ext4
KERNEL_RV     ?= kernel-rv
KERNEL_LA     ?= kernel-la
SDCARD_RV_IMG ?= /mnt/data/os_competition/images/final_img/sdcard-rv-pub.img
SDCARD_LA_IMG ?= /mnt/data/os_competition/images/final_img/sdcard-la-pub.img
RISC_SMP      ?= 8
LOONG_SMP     ?= 12

ifeq ($(QEMU_TIMEOUT),0)
QEMU_RV_RUN := qemu-system-riscv64
QEMU_LA_RUN := qemu-system-loongarch64
else
QEMU_RV_RUN := timeout $(QEMU_TIMEOUT) qemu-system-riscv64
QEMU_LA_RUN := timeout $(QEMU_TIMEOUT) qemu-system-loongarch64
endif

DOCKER_IMAGE ?= docker.educg.net/cg/os-contest:20260510


.DEFAULT_GOAL := all
.NOTPARALLEL:
.PHONY: all prepare-cargo build-rv build-la disk-rv disk-la run-rv run-la \
        debug-rv debug-la gdb-rv gdb-la clean

all: build-rv disk-rv build-la disk-la

prepare-cargo:
	@mkdir -p .cargo
	@cp $(CARGO_ROOT_CONFIG) .cargo/config.toml

build-rv: prepare-cargo
	@$(MAKE) -C $(OS_DIR) elf ARCH=riscv64 MODE=$(MODE) FINAL_TEST=$(FINAL_TEST)
	@cp $(RISC_ELF) kernel-rv

disk-rv: prepare-cargo
	@$(MAKE) -C $(OS_DIR) disk ARCH=riscv64 MODE=$(MODE) FINAL_TEST=$(FINAL_TEST)
	@cp $(RISC_DISK_SRC) disk.img

build-la: prepare-cargo
	@$(MAKE) -C $(OS_DIR) elf ARCH=loongarch64 MODE=$(MODE) FINAL_TEST=$(FINAL_TEST)
	@cp $(LOONG_ELF) kernel-la

disk-la: prepare-cargo
	@$(MAKE) -C $(OS_DIR) disk ARCH=loongarch64 MODE=$(MODE) FINAL_TEST=$(FINAL_TEST)
	@cp $(LOONG_DISK_SRC) disk-la.img

# 模拟官方的指令直接打开，不要动
run-rv: 
	$(QEMU_RV_RUN) -machine virt -kernel $(KERNEL_RV) -m $(MEM) -nographic -smp $(RISC_SMP) -bios default \
		-drive file=$(SDCARD_RV_IMG),if=none,format=raw,id=x0 \
		-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 -no-reboot -snapshot -device virtio-net-device,netdev=net -netdev user,id=net \
		-rtc base=utc \
		-drive file=disk.img,if=none,format=raw,id=x1 -device virtio-blk-device,drive=x1,bus=virtio-mmio-bus.1

run-la: 
	$(QEMU_LA_RUN) -machine virt -kernel $(KERNEL_LA) -m $(MEM) -nographic -smp $(LOONG_SMP) \
		-drive file=$(SDCARD_LA_IMG),if=none,format=raw,id=x0  \
		-device virtio-blk-pci,drive=x0 -no-reboot -snapshot -device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::5555-:5555,hostfwd=udp::5555-:5555  \
		-rtc base=utc \
		-drive file=disk-la.img,if=none,format=raw,id=x1 -device virtio-blk-pci,drive=x1


debug-rv: build-rv disk-rv
	@$(MAKE) -C $(OS_DIR) debug ARCH=riscv64 MODE=$(MODE) FINAL_TEST=$(FINAL_TEST) \
		SMP=$(SMP) MEM=$(MEM)

debug-la: build-la disk-la
	@$(MAKE) -C $(OS_DIR) debug ARCH=loongarch64 MODE=$(MODE) FINAL_TEST=$(FINAL_TEST) \
		SMP=$(SMP) MEM=$(MEM)

gdb-rv:
	@$(MAKE) -C $(OS_DIR) gdb ARCH=riscv64 MODE=$(MODE)

gdb-la:
	@$(MAKE) -C $(OS_DIR) gdb ARCH=loongarch64 MODE=$(MODE)

start_docker:
	docker run --rm -it \
		-v "$(CURDIR)":/mnt/OS_Workspace/submit_repo \
		-v /mnt/data:/mnt/data \
		-w /mnt/OS_Workspace/submit_repo \
		$(DOCKER_IMAGE) \
		/bin/bash


clean:
	@$(MAKE) -C $(OS_DIR) clean
	@rm -f kernel-rv kernel-la disk.img disk-la.img
