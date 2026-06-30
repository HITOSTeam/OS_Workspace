This directory is the riscv64-specific extra overlay for ext4-fs-packer.

It contains target binaries, shared libraries, and rootfs overlay files that
must not be packed into loongarch64 images.

`bin/unshare`, `bin/mount`, and `bin/umount` are real util-linux riscv64/musl
tools from Alpine packages, not wrappers around `/user` binaries or busybox
applets.
