This directory is the loongarch64-specific extra overlay for ext4-fs-packer.

Put loongarch64 binaries, libraries, and rootfs overlay files here. The packer
copies the common `extra/` tree first, then overlays this directory when
`ARCH=loongarch64`.
