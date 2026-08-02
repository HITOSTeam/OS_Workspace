#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::String;
use user::syscall::{CREATE, RDWR, TRUNC, close, getpid, open, syscall, write};

const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_SYMLINKAT: usize = 36;
const SYSCALL_UMOUNT2: usize = 39;
const SYSCALL_MOUNT: usize = 40;
const SYSCALL_OPENAT: usize = 56;
const SYSCALL_OPENAT2: usize = 437;

const AT_FDCWD: isize = -100;

const O_DIRECTORY: u64 = 0x10000;
const O_NOFOLLOW: u64 = 0x20000;
const O_PATH: u64 = 0x200000;
const O_EMPTYPATH: u64 = 1 << 26;

const RESOLVE_NO_XDEV: u64 = 0x01;
const RESOLVE_NO_MAGICLINKS: u64 = 0x02;
const RESOLVE_NO_SYMLINKS: u64 = 0x04;
const RESOLVE_BENEATH: u64 = 0x08;
const RESOLVE_IN_ROOT: u64 = 0x10;
const RESOLVE_CACHED: u64 = 0x20;

const ENOENT: isize = -2;
const E2BIG: isize = -7;
const EAGAIN: isize = -11;
const EXDEV: isize = -18;
const ENOTDIR: isize = -20;
const EINVAL: isize = -22;
const ELOOP: isize = -40;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

fn with_c_path<T>(path: &str, f: impl FnOnce(*const u8) -> T) -> T {
    let mut owned = String::from(path);
    owned.push('\0');
    f(owned.as_ptr())
}

fn with_opt_c_path<T>(path: Option<&str>, f: impl FnOnce(*const u8) -> T) -> T {
    match path {
        Some(path) => with_c_path(path, f),
        None => f(core::ptr::null()),
    }
}

fn linux_openat(dirfd: isize, path: &str, flags: usize) -> isize {
    with_c_path(path, |path_ptr| {
        syscall(
            SYSCALL_OPENAT,
            [dirfd as usize, path_ptr as usize, flags, 0, 0, 0],
        )
    })
}

fn linux_openat2(dirfd: isize, path: &str, how: &OpenHow) -> isize {
    raw_openat2(
        dirfd,
        path,
        how as *const OpenHow as usize,
        core::mem::size_of::<OpenHow>(),
    )
}

fn raw_openat2(dirfd: isize, path: &str, how_ptr: usize, size: usize) -> isize {
    with_c_path(path, |path_ptr| {
        syscall(
            SYSCALL_OPENAT2,
            [dirfd as usize, path_ptr as usize, how_ptr, size, 0, 0],
        )
    })
}

fn linux_mkdir(path: &str) -> isize {
    with_c_path(path, |path_ptr| {
        syscall(
            SYSCALL_MKDIRAT,
            [AT_FDCWD as usize, path_ptr as usize, 0o755, 0, 0, 0],
        )
    })
}

fn linux_symlink(target: &str, link: &str) -> isize {
    with_c_path(target, |target_ptr| {
        with_c_path(link, |link_ptr| {
            syscall(
                SYSCALL_SYMLINKAT,
                [
                    target_ptr as usize,
                    AT_FDCWD as usize,
                    link_ptr as usize,
                    0,
                    0,
                    0,
                ],
            )
        })
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
                    syscall(
                        SYSCALL_MOUNT,
                        [
                            source_ptr as usize,
                            target_ptr as usize,
                            type_ptr as usize,
                            flags,
                            data_ptr as usize,
                            0,
                        ],
                    )
                })
            })
        })
    })
}

fn linux_umount(target: &str) -> isize {
    with_c_path(target, |target_ptr| {
        syscall(SYSCALL_UMOUNT2, [target_ptr as usize, 0, 0, 0, 0, 0])
    })
}

fn create_file(path: &str, data: &[u8]) {
    let fd = open(path, RDWR | CREATE | TRUNC);
    assert!(fd >= 0, "create {path}: {fd}");
    assert_eq!(write(fd as usize, data), data.len() as isize);
    assert_eq!(close(fd as usize), 0);
}

fn expect_open(dirfd: isize, path: &str, flags: u64, resolve: u64) {
    let how = OpenHow {
        flags,
        mode: 0,
        resolve,
    };
    let fd = linux_openat2(dirfd, path, &how);
    assert!(
        fd >= 0,
        "openat2 {path} flags={flags:#x} resolve={resolve:#x}: {fd}"
    );
    assert_eq!(close(fd as usize), 0);
}

fn expect_errno(dirfd: isize, path: &str, flags: u64, resolve: u64, errno: isize) {
    let how = OpenHow {
        flags,
        mode: 0,
        resolve,
    };
    assert_eq!(
        linux_openat2(dirfd, path, &how),
        errno,
        "openat2 {path} flags={flags:#x} resolve={resolve:#x}"
    );
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let root = alloc::format!("/tmp/vfs_pathwalk_smoke_{}", getpid());
    let base = alloc::format!("{root}/base");
    let mountpoint = alloc::format!("{base}/mnt");
    assert_eq!(linux_mkdir(&root), 0);
    assert_eq!(linux_mkdir(&base), 0);
    assert_eq!(linux_mkdir(&mountpoint), 0);

    let inside = alloc::format!("{base}/inside");
    let outside = alloc::format!("{root}/outside");
    create_file(&inside, b"inside");
    create_file(&outside, b"outside");
    assert_eq!(
        linux_symlink("inside", &alloc::format!("{base}/inside-link")),
        0
    );
    assert_eq!(
        linux_symlink("../outside", &alloc::format!("{base}/escape")),
        0
    );
    assert_eq!(
        linux_symlink(&outside, &alloc::format!("{base}/absolute")),
        0
    );

    assert_eq!(
        linux_mount(
            Some("tmpfs"),
            &mountpoint,
            Some("tmpfs"),
            0,
            Some("size=1m,mode=755")
        ),
        0
    );
    create_file(&alloc::format!("{mountpoint}/upper"), b"upper");

    let dirfd = linux_openat(AT_FDCWD, &base, O_DIRECTORY as usize);
    assert!(dirfd >= 0);
    let dirfd = dirfd as isize;

    // Extensible ABI validation and exact errno precedence.
    let how = OpenHow::default();
    assert_eq!(
        raw_openat2(dirfd, "inside", &how as *const _ as usize, 16),
        EINVAL
    );
    assert_eq!(
        raw_openat2(dirfd, "inside", &how as *const _ as usize, 4097),
        E2BIG
    );
    let extended = [0u64, 0, 0, 1];
    assert_eq!(
        raw_openat2(dirfd, "inside", extended.as_ptr() as usize, 32),
        E2BIG
    );
    expect_errno(dirfd, "inside", 1u64 << 63, 0, EINVAL);
    expect_errno(dirfd, "inside", 0, 0x40, EINVAL);
    expect_errno(
        dirfd,
        "inside",
        0,
        RESOLVE_BENEATH | RESOLVE_IN_ROOT,
        EINVAL,
    );
    let invalid_mode = OpenHow {
        flags: 0,
        mode: 0o644,
        resolve: 0,
    };
    assert_eq!(linux_openat2(dirfd, "inside", &invalid_mode), EINVAL);

    // BENEATH rejects every escape spelling while allowing an in-tree open.
    expect_open(dirfd, "inside", 0, RESOLVE_BENEATH);
    expect_errno(dirfd, "..", 0, RESOLVE_BENEATH, EXDEV);
    expect_errno(dirfd, "../outside", 0, RESOLVE_BENEATH, EXDEV);
    expect_errno(dirfd, "escape", 0, RESOLVE_BENEATH, EXDEV);
    expect_errno(dirfd, "absolute", 0, RESOLVE_BENEATH, EXDEV);
    expect_errno(dirfd, &outside, 0, RESOLVE_BENEATH, EXDEV);

    // IN_ROOT treats dirfd as a temporary root for absolute paths and dotdot.
    expect_open(dirfd, "/inside", 0, RESOLVE_IN_ROOT);
    expect_open(dirfd, "/../inside", 0, RESOLVE_IN_ROOT);
    expect_errno(dirfd, &outside, 0, RESOLVE_IN_ROOT, ENOENT);

    // Mount transitions in either direction are rejected by NO_XDEV.
    expect_errno(dirfd, "mnt/upper", 0, RESOLVE_NO_XDEV, EXDEV);
    let mountfd = linux_openat(AT_FDCWD, &mountpoint, O_DIRECTORY as usize);
    assert!(mountfd >= 0);
    expect_errno(mountfd, "..", 0, RESOLVE_NO_XDEV, EXDEV);
    assert_eq!(close(mountfd as usize), 0);

    // NO_SYMLINKS permits opening the final link itself with O_PATH|O_NOFOLLOW.
    expect_errno(dirfd, "inside-link", 0, RESOLVE_NO_SYMLINKS, ELOOP);
    expect_open(
        dirfd,
        "inside-link",
        O_PATH | O_NOFOLLOW,
        RESOLVE_NO_SYMLINKS,
    );
    let proc_magic = alloc::format!("/proc/self/fd/{dirfd}/inside");
    expect_errno(AT_FDCWD, &proc_magic, 0, RESOLVE_NO_MAGICLINKS, ELOOP);

    expect_errno(dirfd, "inside/", 0, 0, ENOTDIR);
    expect_errno(dirfd, "", O_PATH | O_DIRECTORY, 0, ENOENT);
    expect_open(dirfd, "", O_PATH | O_DIRECTORY | O_EMPTYPATH, 0);
    expect_errno(dirfd, "inside", 0, RESOLVE_CACHED, EAGAIN);

    assert_eq!(close(dirfd as usize), 0);
    assert_eq!(linux_umount(&mountpoint), 0);
    println!("VFS_PATHWALK_ERRNO_PASS");
    0
}
