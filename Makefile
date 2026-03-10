SUBMIT_FOLDER := submit_repo
SUBMIT_MAKEFILE := submit_repo.Makefile
SDCARD_RV_IMG := sdcard-rv.img


prepare_submit:
	mkdir -p $(SUBMIT_FOLDER)

# copy os to the submit folder
copy_os: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/os
	mkdir -p $(SUBMIT_FOLDER)/os
	rsync -av --exclude='.git' --exclude='.cargo' --exclude='target' --exclude='*.log' os/ $(SUBMIT_FOLDER)/os/

copy_user: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/user
	mkdir -p $(SUBMIT_FOLDER)/user
	rsync -av --exclude='.git' --exclude='.cargo' --exclude='target' user/ $(SUBMIT_FOLDER)/user/

copy_ext4_fs_packer: prepare_submit
	# dont copy the extra folder to reduce size and target
	rm -rf $(SUBMIT_FOLDER)/ext4-fs-packer
	mkdir -p $(SUBMIT_FOLDER)/ext4-fs-packer
	rsync -av --exclude='.git' --exclude='*.log' --exclude="extra" --exclude="target" ext4-fs-packer/ $(SUBMIT_FOLDER)/ext4-fs-packer/

copy_ext4_fs: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/ext4-fs
	mkdir -p $(SUBMIT_FOLDER)/ext4-fs
	rsync -av --exclude='.git' --exclude='*.log' --exclude="target" ext4-fs/ $(SUBMIT_FOLDER)/ext4-fs/

copy_vendor: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/vendor
	mkdir -p $(SUBMIT_FOLDER)/vendor
	rsync -av --exclude='.git' --exclude='*.log' --exclude="target" vendor/ $(SUBMIT_FOLDER)/vendor/

copy_cargo_config: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/cargo-config
	mkdir -p $(SUBMIT_FOLDER)/cargo-config
	rsync -av --exclude='.git' --exclude='*.log' cargo-config/ $(SUBMIT_FOLDER)/cargo-config/

copy_workspace: prepare_submit
	cp -f cargo.toml $(SUBMIT_FOLDER)/

copy_submit_makefile: prepare_submit
	cp -f $(SUBMIT_MAKEFILE) $(SUBMIT_FOLDER)/Makefile
copy_readme:
	cp -f README_SUBMIT.md $(SUBMIT_FOLDER)/README.md
copy_sdcard: prepare_submit
	@if [ -f "$(SDCARD_RV_IMG)" ]; then \
		cp -f "$(SDCARD_RV_IMG)" "$(SUBMIT_FOLDER)/"; \
	else \
		echo "skip $(SDCARD_RV_IMG) (not found)"; \
	fi
copy_img: prepare_submit
	rm -rf $(SUBMIT_FOLDER)/img
	cp -r img/ $(SUBMIT_FOLDER)/img/

all: copy_os copy_user copy_ext4_fs_packer copy_ext4_fs copy_vendor copy_cargo_config copy_workspace copy_submit_makefile copy_readme copy_sdcard copy_img

clean:
	@if [ -d "$(SUBMIT_FOLDER)/.git" ]; then \
		echo "preserve $(SUBMIT_FOLDER)/.git"; \
		find "$(SUBMIT_FOLDER)" -mindepth 1 -maxdepth 1 ! -name .git -exec rm -rf {} +; \
	else \
		rm -rf $(SUBMIT_FOLDER); \
	fi

.PHONY: all clean prepare_submit copy_os copy_user copy_ext4_fs_packer copy_ext4_fs copy_vendor copy_cargo_config copy_workspace copy_submit_makefile copy_readme copy_sdcard
