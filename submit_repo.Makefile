MODE ?= release
SUBMIT ?= 1

OS_DIR := os
USER_DIR := user
CARGO_CONFIG := cargo-config/config.toml
EXT4_PACKER_DIR := ext4-fs-packer
USER_APP_DIR := os/results
DISK_IMG := disk.img
DISK_SIZE ?= 1G
BASE_IMG ?= img/disk.img
BASE_IMG_TAR ?= img/disk.tar
BASE_IMG_TAR_XZ ?= img/disk.tar.xz
BASE_IMG_ARG :=
ifneq ($(strip $(BASE_IMG)),)
BASE_IMG_ARG := -b $(abspath $(BASE_IMG))
BASE_IMG_DEP := base-img
endif
SDCARD_RV_IMG ?= ./sdcard-rv.img
SMP ?= 1
MEM ?= 1G

# Only append the extra virtio disk if the file exists
ifneq (,$(wildcard $(DISK_IMG)))
DISK_ARGS := -drive file=$(DISK_IMG),if=none,format=raw,id=x1 -device virtio-blk-device,drive=x1,bus=virtio-mmio-bus.1
else
DISK_ARGS :=
endif

RISC_TARGET := riscv64gc-unknown-none-elf
RISC_ELF := $(OS_DIR)/target/$(RISC_TARGET)/$(MODE)/os
LOONGARCH_ELF ?= $(OS_DIR)/target/loongarch64-unknown-none-softfloat/$(MODE)/os

KERNEL_RV := kernel-rv
KERNEL_LA := kernel-la

all: prepare-cargo build-rv build-la disk-img

prepare-cargo:
	@mkdir -p $(OS_DIR)/.cargo $(USER_DIR)/.cargo
	@cp $(CARGO_CONFIG) $(OS_DIR)/.cargo/config.toml
	@cp $(CARGO_CONFIG) $(USER_DIR)/.cargo/config.toml

build-rv: prepare-cargo
	@$(MAKE) -C $(OS_DIR) KERNEL MODE=$(MODE) SUBMIT=$(SUBMIT)
	@cp $(RISC_ELF) $(KERNEL_RV)

build-la: build-rv
	@if [ -f "$(LOONGARCH_ELF)" ]; then \
		cp "$(LOONGARCH_ELF)" "$(KERNEL_LA)"; \
	else \
		echo "warning: loongarch64 kernel not found, copying $(KERNEL_RV) as placeholder"; \
		cp "$(KERNEL_RV)" "$(KERNEL_LA)"; \
	fi

clean:
	@$(MAKE) -C $(OS_DIR) clean
	@rm -f $(KERNEL_RV) $(KERNEL_LA)

disk-img: build-rv $(BASE_IMG_DEP)
	@cd $(EXT4_PACKER_DIR) && cargo run --release -- \
		-u ../$(USER_APP_DIR) \
		$(BASE_IMG_ARG) \
		-t ../ \
		-S $(DISK_SIZE) \
		-o $(DISK_IMG)
	cp disk.img disk-rv.img

base-img:
	@if [ ! -f "$(BASE_IMG)" ]; then \
		if [ -f "$(BASE_IMG_TAR)" ]; then \
			echo "📦 Extracting base image from $(BASE_IMG_TAR)..."; \
			tar -xf "$(BASE_IMG_TAR)" -C "$(dir $(BASE_IMG))"; \
		elif [ -f "$(BASE_IMG_TAR_XZ)" ]; then \
			echo "📦 Extracting base image from $(BASE_IMG_TAR_XZ)..."; \
			tar -xf "$(BASE_IMG_TAR_XZ)" -C "$(dir $(BASE_IMG))"; \
		else \
			echo "❌ Base image not found: $(BASE_IMG)"; \
			exit 1; \
		fi; \
	fi

run-rv: build-rv
	qemu-system-riscv64 -machine virt -kernel $(KERNEL_RV) -m $(MEM) -nographic -smp $(SMP) \
		-bios default \
		-drive file=$(SDCARD_RV_IMG),if=none,format=raw,id=x0 \
		-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \
		-no-reboot \
		-device virtio-net-device,netdev=net -netdev user,id=net \
		-rtc base=utc \
		$(DISK_ARGS)

.PHONY: all prepare-cargo build-rv build-la clean disk-img run-rv base-img
