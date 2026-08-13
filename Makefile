SUBMIT_FOLDER := submit_repo
SUBMIT_MAKEFILE := submit_repo.Makefile
LOCAL_VENDOR_TMP := .local-vendor
VENDOR_DIR := vendor/cargo-vendor
VENDOR_TMP := .vendor-tmp

.DEFAULT_GOAL := all
.NOTPARALLEL:

prepare_submit:
	mkdir -p $(SUBMIT_FOLDER)
	rm -rf $(SUBMIT_FOLDER)/.cargo
	rm -rf $(SUBMIT_FOLDER)/cargo-vendor
	rm -rf $(SUBMIT_FOLDER)/target
	rm -rf $(LOCAL_VENDOR_TMP)
	rm -f $(SUBMIT_FOLDER)/kernel-rv $(SUBMIT_FOLDER)/kernel-la $(SUBMIT_FOLDER)/disk.img $(SUBMIT_FOLDER)/disk-la.img

# copy os to the submit folder
copy_os: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/os
	mkdir -p $(SUBMIT_FOLDER)/os
	tar --exclude='.git' --exclude='.cargo' --exclude='target' --exclude='*.log' -cf - -C os . | tar -xf - -C $(SUBMIT_FOLDER)/os

copy_user: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/user
	mkdir -p $(SUBMIT_FOLDER)/user
	tar --exclude='.git' --exclude='.cargo' --exclude='target' -cf - -C user . | tar -xf - -C $(SUBMIT_FOLDER)/user

copy_ext4_fs_packer: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/ext4-fs-packer
	mkdir -p $(SUBMIT_FOLDER)/ext4-fs-packer
	tar --exclude='.git' --exclude='*.log' --exclude='target' -cf - -C ext4-fs-packer . | tar -xf - -C $(SUBMIT_FOLDER)/ext4-fs-packer

copy_ext4_fs: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/ext4-fs
	mkdir -p $(SUBMIT_FOLDER)/ext4-fs
	tar --exclude='.git' --exclude='*.log' --exclude='target' -cf - -C ext4-fs . | tar -xf - -C $(SUBMIT_FOLDER)/ext4-fs

# copy_easy_fs: prepare_submit
# 	rm -rf $(SUBMIT_FOLDER)/easy-fs
# 	mkdir -p $(SUBMIT_FOLDER)/easy-fs
# 	tar --exclude='.git' --exclude='*.log' --exclude='target' -cf - -C easy-fs . | tar -xf - -C $(SUBMIT_FOLDER)/easy-fs

# copy_easy_fs_fuse: prepare_submit
# 	rm -rf $(SUBMIT_FOLDER)/easy-fs-fuse
# 	mkdir -p $(SUBMIT_FOLDER)/easy-fs-fuse
# 	tar --exclude='.git' --exclude='*.log' --exclude='target' -cf - -C easy-fs-fuse . | tar -xf - -C $(SUBMIT_FOLDER)/easy-fs-fuse

vendor:
	@rm -rf $(VENDOR_TMP)
	@cargo vendor --locked --versioned-dirs $(VENDOR_TMP)
	@rm -rf $(VENDOR_DIR)
	@mkdir -p vendor
	@mv $(VENDOR_TMP) $(VENDOR_DIR)

copy_vendor: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/vendor
	rm -rf $(LOCAL_VENDOR_TMP)
	mkdir -p $(LOCAL_VENDOR_TMP)
	tar --exclude='.git' --exclude='*.log' --exclude='target' -cf - -C vendor . | tar -xf - -C $(LOCAL_VENDOR_TMP)
	# 完整的离线依赖已预先保存在 vendor/cargo-vendor；此处只复制，
	# 避免在无网络环境下重新解析 crates.io 索引。
	mv $(LOCAL_VENDOR_TMP) $(SUBMIT_FOLDER)/vendor

copy_cargo_config: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/cargo-config $(SUBMIT_FOLDER)/Cargo.toml
	mkdir -p $(SUBMIT_FOLDER)/cargo-config
	tar --exclude='.git' --exclude='*.log' -cf - -C cargo-config . | tar -xf - -C $(SUBMIT_FOLDER)/cargo-config


copy_gitignore:
	cp -f ./submit_repo.gitignore $(SUBMIT_FOLDER)/.gitignore

copy_workspace: prepare_submit
	cp -f submit_repo.Cargo.toml $(SUBMIT_FOLDER)/Cargo.toml
	cp -f Cargo.lock $(SUBMIT_FOLDER)/
	cp -f rust-toolchain.toml $(SUBMIT_FOLDER)/

copy_submit_makefile: prepare_submit
	cp -f $(SUBMIT_MAKEFILE) $(SUBMIT_FOLDER)/Makefile
copy_readme: prepare_submit
	cp -f README_SUBMIT.md $(SUBMIT_FOLDER)/README.md
copy_img: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/img
	cp -r img/ $(SUBMIT_FOLDER)/img/

all: copy_os copy_user copy_ext4_fs_packer copy_ext4_fs  copy_vendor copy_cargo_config copy_workspace copy_submit_makefile copy_readme copy_img copy_gitignore
	chmod -R u+rwX,go+rX ./submit_repo

clean:
	@if [ -d "$(SUBMIT_FOLDER)/.git" ]; then \
		echo "preserve $(SUBMIT_FOLDER)/.git"; \
		find "$(SUBMIT_FOLDER)" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +; \
	else \
		rm -rf $(SUBMIT_FOLDER); \
	fi
	@rm -rf $(LOCAL_VENDOR_TMP)
	@cargo clean

.PHONY: all clean vendor prepare_submit copy_os copy_user copy_ext4_fs_packer copy_ext4_fs copy_vendor copy_cargo_config copy_workspace copy_submit_makefile copy_readme copy_img copy_gitignore

# 将 QEMU 使用的原始磁盘镜像写入整张 SD 卡。示例：
#   make qemu-disk-image
#   make sdcard-write SDCARD=/dev/sdX SDCARD_IMAGE=img/disk.img
#
# SDCARD 必须是整盘设备（例如 /dev/sdb），不能是分区（例如 /dev/sdb1）。
# 该目标会拒绝写入仍有挂载分区的设备；请先用 lsblk 确认设备名并手动卸载。
SDCARD ?=
SDCARD_IMAGE ?= $(CURDIR)/img/disk.img
QEMU_RISCV_DISK_ARCHIVE ?= $(CURDIR)/img/disk.tar.xz
VF2_SDCARD_IMAGE ?= $(CURDIR)/visionfive2-dual-disk.img
VF2_LOCAL_DISK ?= $(CURDIR)/ext4-fs-packer/target/fs-riscv64.ext4
VF2_OFFICIAL_DISK ?= /images_host/final_img/sdcard-rv-pub.img

# 评测基础盘在仓库中以 xz 压缩包保存。仅在 disk.img 不存在时解压，
# 避免意外覆盖已经准备好的本地镜像。
qemu-disk-image:
	@set -eu; \
	if [ -f "$(SDCARD_IMAGE)" ]; then \
		echo "QEMU 镜像已存在：$(SDCARD_IMAGE)"; \
	elif [ -f "$(QEMU_RISCV_DISK_ARCHIVE)" ]; then \
		tar -xJkf "$(QEMU_RISCV_DISK_ARCHIVE)" -C "$(dir $(SDCARD_IMAGE))"; \
		echo "已解压 QEMU 镜像：$(SDCARD_IMAGE)"; \
	else \
		echo "找不到 QEMU 镜像或压缩包：$(SDCARD_IMAGE) / $(QEMU_RISCV_DISK_ARCHIVE)"; exit 2; \
	fi

# 生成 VisionFive 2 用的单卡双 GPT 分区镜像。分区 1 是本地 `/user` 盘，
# 分区 2 是官方完整 rootfs；内核将它们注册为两个逻辑块设备。
# 生成 17 GiB sparse 文件，实际占用空间取决于文件系统是否支持稀疏文件。
vf2-sdcard-image:
	@set -eu; \
	if [ ! -f "$(VF2_LOCAL_DISK)" ]; then \
		echo "缺少本地磁盘：$(VF2_LOCAL_DISK)；先执行 make -C os disk ARCH=riscv64 FINAL_TEST=1"; exit 2; \
	fi; \
	if [ ! -f "$(VF2_OFFICIAL_DISK)" ]; then \
		echo "缺少官方镜像：$(VF2_OFFICIAL_DISK)"; exit 2; \
	fi; \
	local_bytes=$$(stat -c %s "$(VF2_LOCAL_DISK)"); official_bytes=$$(stat -c %s "$(VF2_OFFICIAL_DISK)"); \
	if [ "$$local_bytes" -gt $$((1096 * 1024 * 1024)) ] || [ "$$official_bytes" -gt $$((15900 * 1024 * 1024)) ]; then \
		echo "镜像超出固定分区容量；请调整 vf2-sdcard-image 的布局。"; exit 2; \
	fi; \
	rm -f "$(VF2_SDCARD_IMAGE)"; \
	truncate -s 17G "$(VF2_SDCARD_IMAGE)"; \
	parted -s "$(VF2_SDCARD_IMAGE)" mklabel gpt; \
	parted -s "$(VF2_SDCARD_IMAGE)" mkpart local-ext4 4MiB 1100MiB; \
	parted -s "$(VF2_SDCARD_IMAGE)" mkpart official-ext4 1100MiB 100%; \
	dd if="$(VF2_LOCAL_DISK)" of="$(VF2_SDCARD_IMAGE)" bs=4M seek=1 conv=notrunc status=progress; \
	dd if="$(VF2_OFFICIAL_DISK)" of="$(VF2_SDCARD_IMAGE)" bs=4M seek=275 conv=notrunc status=progress; \
	sync; \
	echo "已生成：$(VF2_SDCARD_IMAGE)"; \
	echo "手动写卡：make sdcard-write SDCARD=/dev/sdX SDCARD_IMAGE=$(VF2_SDCARD_IMAGE)"

# 决赛一键版本：先用 submit 用户程序制作本地盘，再合成双分区 SD 卡镜像。
# 本质是协调两个makefile里面的信息,先使用本地的打包代码制作user镜像,然后使用烧录程序把镜像烧录
# 使用$(make)会继承当前makefile里面的一些变量 --no-print-directory是不打印当前的镜像
vf2-final-sdcard-image:
#首先使用os目录里面的那个makefile创建我们自己的镜像  
	@$(MAKE) -C os disk ARCH=riscv64 FINAL_TEST=1 MODE=release
# 使用当前makefile创建
	@$(MAKE) --no-print-directory vf2-sdcard-image

sdcard-info:
	@echo "使用前请确认 SD 卡的整盘设备名（不要填分区）："
	@lsblk -o NAME,SIZE,TYPE,MODEL,TRAN,MOUNTPOINTS

# 写镜像的部分
sdcard-write:
	@set -eu; \
	device="$(SDCARD)"; image="$(SDCARD_IMAGE)"; \
	if [ -z "$$device" ]; then \
		echo "缺少 SDCARD。示例：make sdcard-write SDCARD=/dev/sdX SDCARD_IMAGE=img/disk.img"; exit 2; \
	fi; \
	if [ ! -b "$$device" ] || ! lsblk -dn -o TYPE "$$device" | grep -qx disk; then \
		echo "SDCARD 必须是存在的整盘块设备（例如 /dev/sdb），当前为：$$device"; exit 2; \
	fi; \
	if [ ! -f "$$image" ]; then \
		echo "找不到镜像：$$image"; exit 2; \
	fi; \
	if lsblk -nr -o MOUNTPOINTS "$$device" | grep -q '[^[:space:]]'; then \
		echo "拒绝写入：$$device 或其分区仍处于挂载状态。请先卸载后重试。"; \
		lsblk -o NAME,SIZE,TYPE,MOUNTPOINTS "$$device"; \
		echo "推荐操作（只会卸载该 SD 卡上的挂载点）："; \
		lsblk -nr -o PATH,MOUNTPOINTS "$$device" | while IFS=' ' read -r path mountpoint; do \
			[ -n "$$mountpoint" ] && echo "  sudo umount $$path"; \
		done; \
		echo "卸载后重新执行原命令。"; exit 2; \
	fi; \
	echo "即将把 $$image 写入 $$device（这会覆盖整张 SD 卡）。"; \
	lsblk -o NAME,SIZE,TYPE,MODEL,TRAN "$$device"; \
	sudo dd if="$$image" of="$$device" bs=4M status=progress conv=fsync; \
	sync; \
	echo "写入完成。请安全弹出 SD 卡后再插入开发板。"

.PHONY: qemu-disk-image vf2-sdcard-image vf2-final-sdcard-image sdcard-info sdcard-write
