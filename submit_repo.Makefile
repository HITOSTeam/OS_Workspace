MODE ?= release
SUBMIT ?= 1
EXT4_REBUILD ?= 1

OS_DIR := os
USER_DIR := user
CARGO_CONFIG := cargo-config/config.toml
CARGO_ROOT_CONFIG := cargo-config/root-config.toml
EXT4_IMG := ext4-fs-packer/target/fs.ext4
SDCARD_RV_IMG ?= sdcard-rv.img
SDCARD_LA_IMG ?= sdcard-la.img
SMP ?= 1
MEM ?= 1G
DOCKER_IMAGE ?= docker.educg.net/cg/os-contest:20260510


# Only append the extra virtio disk if the file exists
ifneq (,$(wildcard disk.img))
DISK_ARGS := -drive file=disk.img,if=none,format=raw,id=x1 -device virtio-blk-device,drive=x1,bus=virtio-mmio-bus.1
else
DISK_ARGS :=
endif

RISC_TARGET := riscv64gc-unknown-none-elf
LOONGARCH_TARGET := loongarch64-unknown-none
RISC_ELF := target/$(RISC_TARGET)/$(MODE)/os
LOONGARCH_ELF := target/$(LOONGARCH_TARGET)/$(MODE)/os

KERNEL_RV := kernel-rv
KERNEL_LA := kernel-la
DISK_RV := disk.img
DISK_LA := disk-la.img

all: prepare-cargo build-rv build-la disk-rv disk-la

prepare-cargo:
	@mkdir -p .cargo $(OS_DIR)/.cargo $(USER_DIR)/.cargo
	@cp $(CARGO_ROOT_CONFIG) .cargo/config.toml
	@cp $(CARGO_CONFIG) $(OS_DIR)/.cargo/config.toml
	@cp $(CARGO_CONFIG) $(USER_DIR)/.cargo/config.toml

build-rv: prepare-cargo
	@$(MAKE) -C $(OS_DIR) ARCH=riscv64 MODE=$(MODE) SUBMIT=$(SUBMIT) kernel
	@cp $(RISC_ELF) $(KERNEL_RV)

build-la: prepare-cargo
	@$(MAKE) -C $(OS_DIR) ARCH=loongarch64 MODE=$(MODE) SUBMIT=$(SUBMIT) kernel
	@cp $(LOONGARCH_ELF) $(KERNEL_LA)

clean:
	@$(MAKE) -C $(OS_DIR) clean
	@rm -f $(KERNEL_RV) $(KERNEL_LA) $(DISK_RV) $(DISK_LA)

disk-rv: prepare-cargo
	@$(MAKE) -C $(OS_DIR) ARCH=riscv64 MODE=$(MODE) SUBMIT=$(SUBMIT) EXT4_REBUILD=$(EXT4_REBUILD) ext4_img
	@cp $(EXT4_IMG) $(DISK_RV)

disk-la: prepare-cargo
	@$(MAKE) -C $(OS_DIR) ARCH=loongarch64 MODE=$(MODE) SUBMIT=$(SUBMIT) EXT4_REBUILD=$(EXT4_REBUILD) ext4_img
	@cp $(EXT4_IMG) $(DISK_LA)

run-rv: 
	qemu-system-riscv64 -machine virt -kernel ${KERNEL_RV} -m $(MEM) -nographic -smp $(SMP) -bios default \
		-drive file=$(SDCARD_RV_IMG),if=none,format=raw,id=x0 \
		-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 -no-reboot -device virtio-net-device,netdev=net -netdev user,id=net \
		-rtc base=utc \
		-drive file=disk.img,if=none,format=raw,id=x1 -device virtio-blk-device,drive=x1,bus=virtio-mmio-bus.1

run-la: 
	qemu-system-loongarch64 -kernel ${KERNEL_LA} -m $(MEM) -nographic -smp 1 \
		-drive file=$(SDCARD_LA_IMG),if=none,format=raw,id=x0  \
		-device virtio-blk-pci,drive=x0 -no-reboot  -device virtio-net-pci,netdev=net0 \
		-netdev user,id=net0,hostfwd=tcp::5555-:5555,hostfwd=udp::5555-:5555  \
		-rtc base=utc \
		-drive file=disk-la.img,if=none,format=raw,id=x1 -device virtio-blk-pci,drive=x1


start-docker:
	docker run --rm -it \
		-v "$(CURDIR)":/mnt/OS_Workspace/submit_repo \
		-w /mnt/OS_Workspace/submit_repo \
		$(DOCKER_IMAGE) \
		/bin/bash

.PHONY: all prepare-cargo build-rv build-la clean disk-rv disk-la run-rv run-la start-docker
