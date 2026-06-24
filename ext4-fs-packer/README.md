# ext4-fs-packer

## 新增的文件说明

### 1. extra/libcyclictest_sched_loongarch_fix.c

- musl 的loongarch库里面对schedule相关的几个函数都不是走系统调用的，而是直接返回的`ENOSYS`

- 查看对应的系统调用



This tool creates an ext4 filesystem image with a structured layout:

**ATTENTION**:This lib is just a tool for the RUSTOS competion. I dont maintain the git healthily.
The doc might not be consistent with the code in time.

```
/user   - User binaries (required)
/extra  - Additional files (optional)
```

It uses the system `mke2fs -d` command from `e2fsprogs` to populate the image.

## Requirements

- Linux host
- `mke2fs` available (install `e2fsprogs` on Debian/Ubuntu)

## Usage

### Build

```sh
cd ext4-fs-packer
cargo build --release
```

### Create an image

```sh
# Basic: pack user binaries only
./target/release/ext4-fs-packer -u /path/to/user_binaries -t /path/to/output -S 64M

# With extra files
./target/release/ext4-fs-packer -u /path/to/user_binaries -e /path/to/extra_files -t /path/to/output -S 64M

# Based on an existing ext4 image
./target/release/ext4-fs-packer -u /path/to/user_binaries \
    -e /path/to/extra_files \
    -b /path/to/base.ext4 \
    -t /path/to/output -S 64M
```

### Options

| Option                | Description                                            |
| --------------------- | ------------------------------------------------------ |
| `-u, --user <DIR>`    | Directory containing user binaries (placed in `/user`) |
| `-e, --extra <DIR>`   | Optional extra directory (placed in `/extra`)          |
| `-b, --base-image <IMG>` | Optional base ext4 image to seed contents           |
| `-t, --target <DIR>`  | Output directory for the image                         |
| `-S, --size <SIZE>`   | Image size (e.g., 16M, 64M, 1G). Default: 64M          |
| `-o, --output <NAME>` | Output filename. Default: fs.ext4                      |

### Example with CongCore

```sh
# From the project root, pack user binaries
cd ext4-fs-packer
cargo run --release -- -u ../user/target/riscv64gc-unknown-none-elf/release -t target -S 64M

# Or with testsuits
cargo run --release -- -u ../user/target/riscv64gc-unknown-none-elf/release \
    -e ../testsuits-for-oskernel/sdcard \
    -t target -S 128M
```

## Notes

- The packer uses `mke2fs -d` which copies contents into the image at creation time
- No root privileges or loop device mounting required
- If `mke2fs` is not present, install `e2fsprogs`:
  ```sh
  sudo apt install e2fsprogs
  ```
