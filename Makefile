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
