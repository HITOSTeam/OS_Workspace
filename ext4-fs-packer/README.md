# ext4-fs-packer

This tool creates one of three ext4 layouts:

- `system`: a system root seeded from a base image and overlays, without user
  binaries;
- `user`: a standalone filesystem mounted at `/user`; user binaries are stored
  at the image root;
- `combined`: the legacy layout containing both the system root and `/user`.

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
# Standalone /user filesystem
./target/release/ext4-fs-packer --kind user \
    -u /path/to/user_binaries -t /path/to/output \
    -o user.ext4 -L congcore-user -S 256M

# System root without /user binaries
./target/release/ext4-fs-packer --kind system \
    -e /path/to/extra_common \
    --arch-extra /path/to/extra-riscv64 \
    -b /path/to/base.ext4 \
    -t /path/to/output -o system.ext4 -L congcore-system -S 4G

# Legacy combined image
./target/release/ext4-fs-packer --kind combined \
    -u /path/to/user_binaries -e /path/to/extra_common \
    -b /path/to/base.ext4 -t /path/to/output -S 4G
```

### Options

| Option                | Description                                            |
| --------------------- | ------------------------------------------------------ |
| `--kind <KIND>`       | `combined`, `system`, or `user`; default: `combined`   |
| `-u, --user <DIR>`    | User binaries; required by `combined` and `user`       |
| `-e, --extra <DIR>`   | Common system overlay (placed in `/extra`)             |
| `--arch-extra <DIR>`  | Optional arch-specific extra overlay copied after `--extra` |
| `-b, --base-image <IMG>` | Optional base ext4 image to seed contents           |
| `-t, --target <DIR>`  | Output directory for the image                         |
| `-S, --size <SIZE>`   | Image size (e.g., 16M, 64M, 1G). Default: 64M          |
| `-o, --output <NAME>` | Output filename. Default: fs.ext4                      |
| `-L, --label <LABEL>` | Optional ext4 volume label                             |

### Example with CongCore

```sh
make -C os system_img ARCH=riscv64 EXT4_SIZE=4G
make -C os user_img ARCH=riscv64 USER_EXT4_SIZE=256M
```

The CongCore build uses `ext4-fs-packer/extra` for architecture-neutral text
configuration and `ext4-fs-packer/extra-$(ARCH)` for target-specific binaries
and libraries. For example, riscv64 command tools live in `extra-riscv64`, while
loongarch64-specific overlays should live in `extra-loongarch64`.

## Notes

- The packer uses `mke2fs -d` which copies contents into the image at creation time
- Seeding a system/combined image from `--base-image` also requires `debugfs`
- No root privileges or loop device mounting required
- If `mke2fs` is not present, install `e2fsprogs`:
  ```sh
  sudo apt install e2fsprogs
  ```
