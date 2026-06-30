This directory is the loongarch64-specific extra overlay for ext4-fs-packer.

Put loongarch64 binaries, libraries, and rootfs overlay files here. The packer
copies the common `extra/` tree first, then overlays this directory when
`ARCH=loongarch64`.

Most command binaries and shared libraries here are loong64 Debian ports
packages from the 2025-08-16 sid snapshot, selected to match the glibc 2.41
base image. The static `bin/busybox` copy is taken from `sdcard-la.img` and is
used only for applet-style fallbacks whose Debian builds pull in large extra
dependency chains, such as `ss`, `pgrep`, `pkill`, `sysctl`, `telnet`, and
`rm`.

`rootfs/lib/libnss_files.so.2` intentionally overrides the common overlay's
riscv64 NSS library with the loongarch64 glibc 2.41 copy.
