# VFS host tests

This harness compiles the architecture-independent tests embedded in
`os/src/fs/vfs/mod.rs` and `os/src/fs/tmpfs/mod.rs` on the host:

```sh
HOST_TARGET="$(rustc +nightly-2026-07-15 -vV | sed -n 's/^host: //p')" \
CARGO_BUILD_TARGET="$HOST_TARGET" \
CARGO_TARGET_DIR="$PWD/.tmp/vfs-host-tests" \
  cargo +nightly-2026-07-15 test --offline \
  --locked --manifest-path tools/vfs-host-tests/Cargo.toml
```

It replaces only `UserBuffer`, time, and the legacy `File` trait shell.  VFS,
mount graph, path walk, and tmpfs implementation code is not copied.
