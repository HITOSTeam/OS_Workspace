#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::{String, ToString};
use core::str;
use user::syscall::{
    CREATE, RDONLY, RDWR, close, exit, fork, getpid, open, pipe, read, syscall, waitpid, write,
};

const SYSCALL_OPENAT: usize = 56;
const SYSCALL_READLINKAT: usize = 78;
const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_UNLINKAT: usize = 35;
const SYSCALL_MOUNT: usize = 40;
const SYSCALL_UMOUNT2: usize = 39;
const SYSCALL_UNSHARE: usize = 97;
const SYSCALL_SETNS: usize = 268;

const AT_FDCWD: isize = -100;
const AT_REMOVEDIR: usize = 0x200;

const O_NOFOLLOW: usize = 0x20000;
const O_PATH: usize = 0x200000;

const CLONE_NEWNS: usize = 0x0002_0000;
const MS_BIND: usize = 0x1000;
const MS_REC: usize = 1 << 14;
const MS_UNBINDABLE: usize = 1 << 17;
const MS_PRIVATE: usize = 1 << 18;
const MS_SLAVE: usize = 1 << 19;
const MS_SHARED: usize = 1 << 20;

const EEXIST: isize = -17;
const EINVAL: isize = -22;

#[derive(Clone, Copy)]
enum PropagationCase {
    Shared,
    Private,
    Slave,
}

struct MountTestPaths {
    src: String,
    dst: String,
    dira: String,
    dirb: String,
    dira_a: String,
    dira_b: String,
    dirb_b: String,
}

fn with_c_path<T>(path: &str, f: impl FnOnce(*const u8) -> T) -> T {
    let mut owned = String::from(path);
    owned.push('\0');
    f(owned.as_ptr())
}

fn with_opt_c_path<T>(path: Option<&str>, f: impl FnOnce(*const u8) -> T) -> T {
    if let Some(path) = path {
        with_c_path(path, f)
    } else {
        f(core::ptr::null())
    }
}

fn linux_openat(dirfd: isize, path: &str, flags: usize, mode: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(SYSCALL_OPENAT, [
            dirfd as usize,
            ptr as usize,
            flags,
            mode,
            0,
            0,
        ])
    })
}

fn linux_readlinkat(dirfd: isize, path: &str, buf: &mut [u8]) -> isize {
    with_c_path(path, |ptr| {
        syscall(SYSCALL_READLINKAT, [
            dirfd as usize,
            ptr as usize,
            buf.as_mut_ptr() as usize,
            buf.len(),
            0,
            0,
        ])
    })
}

fn linux_mkdirat(dirfd: isize, path: &str, mode: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(SYSCALL_MKDIRAT, [
            dirfd as usize,
            ptr as usize,
            mode,
            0,
            0,
            0,
        ])
    })
}

fn linux_unlinkat(dirfd: isize, path: &str, flags: usize) -> isize {
    with_c_path(path, |ptr| {
        syscall(SYSCALL_UNLINKAT, [
            dirfd as usize,
            ptr as usize,
            flags,
            0,
            0,
            0,
        ])
    })
}

fn linux_mount(
    source: Option<&str>,
    target: &str,
    fs_type: Option<&str>,
    flags: usize,
    data: Option<&str>,
) -> isize {
    with_opt_c_path(source, |source_ptr| {
        with_c_path(target, |target_ptr| {
            with_opt_c_path(fs_type, |type_ptr| {
                with_opt_c_path(data, |data_ptr| {
                    syscall(SYSCALL_MOUNT, [
                        source_ptr as usize,
                        target_ptr as usize,
                        type_ptr as usize,
                        flags,
                        data_ptr as usize,
                        0,
                    ])
                })
            })
        })
    })
}

fn linux_unshare(flags: usize) -> isize {
    syscall(SYSCALL_UNSHARE, [flags, 0, 0, 0, 0, 0])
}

fn linux_setns(fd: isize, nstype: usize) -> isize {
    syscall(SYSCALL_SETNS, [fd as usize, nstype, 0, 0, 0, 0])
}

fn linux_umount2(target: &str, flags: usize) -> isize {
    with_c_path(target, |ptr| {
        syscall(SYSCALL_UMOUNT2, [ptr as usize, flags, 0, 0, 0, 0])
    })
}

fn readlink(path: &str) -> String {
    let mut buf = [0u8; 256];
    let len = linux_readlinkat(AT_FDCWD, path, &mut buf);
    assert!(len >= 0);
    str::from_utf8(&buf[..len as usize]).unwrap().to_string()
}

fn read_all(path: &str) -> String {
    let fd = open(path, RDONLY);
    assert!(fd >= 0);
    let fd = fd as usize;

    let mut out = [0u8; 2048];
    let len = read(fd, &mut out);
    assert!(len >= 0);
    assert_eq!(close(fd), 0);
    str::from_utf8(&out[..len as usize]).unwrap().to_string()
}

fn assert_symlink_fd_target(fd: usize, expected_target: &str) {
    let mut buf = [0u8; 256];
    let len = linux_readlinkat(fd as isize, "", &mut buf);
    assert!(len >= 0);
    let target = str::from_utf8(&buf[..len as usize]).unwrap();
    assert_eq!(target, expected_target);
}

fn path_exists(path: &str) -> bool {
    let fd = open(path, RDONLY);
    if fd < 0 {
        return false;
    }
    assert_eq!(close(fd as usize), 0);
    true
}

fn create_file(path: &str) {
    let fd = open(path, CREATE | RDWR);
    assert!(fd >= 0);
    assert_eq!(close(fd as usize), 0);
}

fn proc_mounts_contains_target(mounts: &str, target: &str) -> bool {
    mounts
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .any(|entry| entry == target)
}

fn mkdir_unique(path: &str) {
    let rc = linux_mkdirat(AT_FDCWD, path, 0o755);
    assert!(rc == 0 || rc == EEXIST);
}

fn init_mount_test_paths(pid: isize) -> MountTestPaths {
    let src = alloc::format!("/tmp/mount_ns_smoke_src_{}", pid);
    let dst = alloc::format!("/tmp/mount_ns_smoke_dst_{}", pid);
    let dira = alloc::format!("/tmp/mount_ns_smoke_a_{}", pid);
    let dirb = alloc::format!("/tmp/mount_ns_smoke_b_{}", pid);
    let dira_a = alloc::format!("{}/A", dira);
    let dira_b = alloc::format!("{}/B", dira);
    let dirb_b = alloc::format!("{}/B", dirb);

    mkdir_unique(&src);
    mkdir_unique(&dst);
    mkdir_unique(&dira);
    mkdir_unique(&dirb);
    create_file(&dira_a);
    create_file(&dirb_b);

    MountTestPaths {
        src,
        dst,
        dira,
        dirb,
        dira_a,
        dira_b,
        dirb_b,
    }
}

fn cleanup_mount_test_paths(paths: &MountTestPaths) {
    assert_eq!(linux_unlinkat(AT_FDCWD, &paths.dira_a, 0), 0);
    assert_eq!(linux_unlinkat(AT_FDCWD, &paths.dirb_b, 0), 0);
    assert_eq!(linux_unlinkat(AT_FDCWD, &paths.dst, AT_REMOVEDIR), 0);
    assert_eq!(linux_unlinkat(AT_FDCWD, &paths.src, AT_REMOVEDIR), 0);
    assert_eq!(linux_unlinkat(AT_FDCWD, &paths.dira, AT_REMOVEDIR), 0);
    assert_eq!(linux_unlinkat(AT_FDCWD, &paths.dirb, AT_REMOVEDIR), 0);
}

fn assert_visible(paths: &MountTestPaths, expect_a: bool, expect_b: bool) {
    assert_eq!(path_exists(&paths.dira_a), expect_a);
    assert_eq!(path_exists(&paths.dira_b), expect_b);
}

fn write_byte(fd: usize, value: u8) {
    let buf = [value];
    assert_eq!(write(fd, &buf), 1);
}

fn read_byte(fd: usize) -> u8 {
    let mut buf = [0u8; 1];
    assert_eq!(read(fd, &mut buf), 1);
    buf[0]
}

fn enter_private_mount_namespace() {
    assert_eq!(linux_unshare(CLONE_NEWNS), 0);
    assert_eq!(
        linux_mount(Some("none"), "/", Some("none"), MS_REC | MS_PRIVATE, None),
        0
    );
}

fn prepare_mount_target(paths: &MountTestPaths) {
    assert_eq!(
        linux_mount(Some(&paths.dira), &paths.dira, None, MS_BIND, None),
        0
    );
}

fn restore_mount_namespace(old_ns_fd: usize) {
    assert_eq!(linux_setns(old_ns_fd as isize, CLONE_NEWNS), 0);
}

fn run_namespace_visibility_smoke(
    old_ns_fd: usize,
    mnt_link_fd: usize,
    old_target: &str,
    paths: &MountTestPaths,
) {
    assert_eq!(linux_unshare(CLONE_NEWNS), 0);
    let new_target = readlink("/proc/self/ns/mnt");
    assert_ne!(new_target, old_target);
    assert_symlink_fd_target(mnt_link_fd, &new_target);

    assert_eq!(
        linux_mount(Some(&paths.src), &paths.dst, None, MS_BIND, None),
        0
    );
    let mounts = read_all("/proc/self/mounts");
    assert!(proc_mounts_contains_target(&mounts, &paths.dst));

    restore_mount_namespace(old_ns_fd);
    let restored_target = readlink("/proc/self/ns/mnt");
    assert_eq!(restored_target, old_target);
    assert_symlink_fd_target(mnt_link_fd, &restored_target);

    let restored_mounts = read_all("/proc/self/mounts");
    assert!(!proc_mounts_contains_target(&restored_mounts, &paths.dst));
}

fn run_propagation_case(old_ns_fd: usize, paths: &MountTestPaths, case: PropagationCase) {
    enter_private_mount_namespace();
    prepare_mount_target(paths);

    match case {
        PropagationCase::Shared | PropagationCase::Slave => {
            assert_eq!(
                linux_mount(Some("none"), &paths.dira, Some("none"), MS_SHARED, None),
                0
            );
        }
        PropagationCase::Private => {
            assert_eq!(
                linux_mount(Some("none"), &paths.dira, Some("none"), MS_PRIVATE, None),
                0
            );
        }
    }

    let mut parent_to_child = [0usize; 2];
    let mut child_to_parent = [0usize; 2];
    assert_eq!(pipe(&mut parent_to_child), 0);
    assert_eq!(pipe(&mut child_to_parent), 0);

    let child = fork();
    assert!(child >= 0);
    if child == 0 {
        assert_eq!(close(parent_to_child[1]), 0);
        assert_eq!(close(child_to_parent[0]), 0);
        assert_eq!(linux_unshare(CLONE_NEWNS), 0);
        if matches!(case, PropagationCase::Slave) {
            assert_eq!(
                linux_mount(Some("none"), &paths.dira, Some("none"), MS_SLAVE, None),
                0
            );
        }

        write_byte(child_to_parent[1], b'R');

        assert_eq!(read_byte(parent_to_child[0]), b'1');
        match case {
            PropagationCase::Shared | PropagationCase::Slave => assert_visible(paths, false, true),
            PropagationCase::Private => assert_visible(paths, true, false),
        }
        write_byte(child_to_parent[1], b'1');

        assert_eq!(read_byte(parent_to_child[0]), b'2');
        assert_visible(paths, true, false);

        assert_eq!(
            linux_mount(Some(&paths.dirb), &paths.dira, None, MS_BIND, None),
            0
        );
        write_byte(child_to_parent[1], b'2');

        assert_eq!(read_byte(parent_to_child[0]), b'3');
        assert_eq!(linux_umount2(&paths.dira, 0), 0);
        write_byte(child_to_parent[1], b'3');

        assert_eq!(close(parent_to_child[0]), 0);
        assert_eq!(close(child_to_parent[1]), 0);
        exit(0);
    }

    assert_eq!(close(parent_to_child[0]), 0);
    assert_eq!(close(child_to_parent[1]), 0);

    assert_eq!(read_byte(child_to_parent[0]), b'R');

    assert_eq!(
        linux_mount(Some(&paths.dirb), &paths.dira, None, MS_BIND, None),
        0
    );
    write_byte(parent_to_child[1], b'1');
    assert_eq!(read_byte(child_to_parent[0]), b'1');

    // Stronger than LTP: verify umount pops the propagated top layer and reveals the base mount.
    assert_eq!(linux_umount2(&paths.dira, 0), 0);
    write_byte(parent_to_child[1], b'2');
    assert_eq!(read_byte(child_to_parent[0]), b'2');

    match case {
        PropagationCase::Shared => assert_visible(paths, false, true),
        PropagationCase::Private | PropagationCase::Slave => assert_visible(paths, true, false),
    }

    write_byte(parent_to_child[1], b'3');
    assert_eq!(read_byte(child_to_parent[0]), b'3');
    assert_visible(paths, true, false);

    let mut status = 0i32;
    assert_eq!(waitpid(child as isize, &mut status), child);
    assert_eq!(status, 0);

    assert_eq!(linux_umount2(&paths.dira, 0), 0);
    assert_eq!(close(parent_to_child[1]), 0);
    assert_eq!(close(child_to_parent[0]), 0);
    restore_mount_namespace(old_ns_fd);
}

fn run_unbindable_case(old_ns_fd: usize, paths: &MountTestPaths) {
    enter_private_mount_namespace();
    prepare_mount_target(paths);
    assert_eq!(
        linux_mount(Some("none"), &paths.dira, Some("none"), MS_UNBINDABLE, None),
        0
    );
    assert_eq!(
        linux_mount(Some(&paths.dira), &paths.dirb, None, MS_BIND, None),
        EINVAL
    );
    assert_eq!(linux_umount2(&paths.dira, 0), 0);
    restore_mount_namespace(old_ns_fd);
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let pid = getpid();
    let paths = init_mount_test_paths(pid);

    let old_target = readlink("/proc/self/ns/mnt");
    let old_ns_fd = open("/proc/self/ns/mnt", RDONLY);
    assert!(old_ns_fd >= 0);
    let old_ns_fd = old_ns_fd as usize;

    let mnt_link_fd = linux_openat(AT_FDCWD, "/proc/self/ns/mnt", O_PATH | O_NOFOLLOW, 0);
    assert!(mnt_link_fd >= 0);
    let mnt_link_fd = mnt_link_fd as usize;
    assert_symlink_fd_target(mnt_link_fd, &old_target);

    run_namespace_visibility_smoke(old_ns_fd, mnt_link_fd, &old_target, &paths);
    run_propagation_case(old_ns_fd, &paths, PropagationCase::Shared);
    run_propagation_case(old_ns_fd, &paths, PropagationCase::Private);
    run_propagation_case(old_ns_fd, &paths, PropagationCase::Slave);
    run_unbindable_case(old_ns_fd, &paths);

    assert_eq!(close(old_ns_fd), 0);
    assert_eq!(close(mnt_link_fd), 0);
    cleanup_mount_test_paths(&paths);

    println!("mount_namespace_smoke passed");
    0
}
