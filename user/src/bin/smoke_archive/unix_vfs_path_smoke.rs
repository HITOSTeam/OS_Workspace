#![no_std]
#![no_main]

extern crate alloc;

#[macro_use]
extern crate user;

use alloc::string::String;
use user::syscall::{close, getpid, read, syscall, write};

const SYSCALL_MKDIRAT: usize = 34;
const SYSCALL_UNLINKAT: usize = 35;
const SYSCALL_LINKAT: usize = 37;
const SYSCALL_RENAMEAT: usize = 38;
const SYSCALL_UMOUNT2: usize = 39;
const SYSCALL_MOUNT: usize = 40;
const SYSCALL_CHDIR: usize = 49;
const SYSCALL_SOCKET: usize = 198;
const SYSCALL_BIND: usize = 200;
const SYSCALL_LISTEN: usize = 201;
const SYSCALL_ACCEPT: usize = 202;
const SYSCALL_CONNECT: usize = 203;
const SYSCALL_SENDTO: usize = 206;
const SYSCALL_STATX: usize = 291;

const AT_FDCWD: isize = -100;
const AT_REMOVEDIR: usize = 0x200;

const AF_UNIX: usize = 1;
const SOCK_STREAM: usize = 1;
const SOCK_DGRAM: usize = 2;

const MS_RDONLY: usize = 1;
const MS_BIND: usize = 0x1000;

const STATX_TYPE: usize = 0x0001;
const S_IFMT: u16 = 0o170000;
const S_IFSOCK: u16 = 0o140000;

const ENOENT: isize = -2;
const EBUSY: isize = -16;
const EROFS: isize = -30;
const EPROTOTYPE: isize = -91;
const EADDRINUSE: isize = -98;
const ECONNREFUSED: isize = -111;

#[repr(C)]
#[derive(Clone, Copy)]
struct SockAddrUn {
    family: u16,
    path: [u8; 108],
}

impl SockAddrUn {
    fn pathname(pathname: &str) -> Self {
        let bytes = pathname.as_bytes();
        assert!(bytes.len() < 108);
        let mut path = [0u8; 108];
        path[..bytes.len()].copy_from_slice(bytes);
        Self {
            family: AF_UNIX as u16,
            path,
        }
    }

    fn abstract_name(name: &[u8]) -> Self {
        assert!(!name.is_empty() && name.len() < 107);
        let mut path = [0u8; 108];
        path[1..name.len() + 1].copy_from_slice(name);
        Self {
            family: AF_UNIX as u16,
            path,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct StatxTimestamp {
    seconds: i64,
    nanoseconds: u32,
    reserved: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Statx {
    mask: u32,
    block_size: u32,
    attributes: u64,
    link_count: u32,
    uid: u32,
    gid: u32,
    mode: u16,
    spare0: u16,
    inode: u64,
    size: u64,
    blocks: u64,
    attributes_mask: u64,
    access_time: StatxTimestamp,
    birth_time: StatxTimestamp,
    change_time: StatxTimestamp,
    modify_time: StatxTimestamp,
    rdev_major: u32,
    rdev_minor: u32,
    dev_major: u32,
    dev_minor: u32,
    mount_id: u64,
    direct_io_memory_alignment: u32,
    direct_io_offset_alignment: u32,
    spare3: [u64; 12],
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

fn linux_mkdir(path: &str) -> isize {
    with_c_path(path, |path| {
        syscall(
            SYSCALL_MKDIRAT,
            [AT_FDCWD as usize, path as usize, 0o755, 0, 0, 0],
        )
    })
}

fn linux_unlink(path: &str, flags: usize) -> isize {
    with_c_path(path, |path| {
        syscall(
            SYSCALL_UNLINKAT,
            [AT_FDCWD as usize, path as usize, flags, 0, 0, 0],
        )
    })
}

fn linux_link(old: &str, new: &str) -> isize {
    with_c_path(old, |old| {
        with_c_path(new, |new| {
            syscall(
                SYSCALL_LINKAT,
                [
                    AT_FDCWD as usize,
                    old as usize,
                    AT_FDCWD as usize,
                    new as usize,
                    0,
                    0,
                ],
            )
        })
    })
}

fn linux_rename(old: &str, new: &str) -> isize {
    with_c_path(old, |old| {
        with_c_path(new, |new| {
            syscall(
                SYSCALL_RENAMEAT,
                [
                    AT_FDCWD as usize,
                    old as usize,
                    AT_FDCWD as usize,
                    new as usize,
                    0,
                    0,
                ],
            )
        })
    })
}

fn linux_chdir(path: &str) -> isize {
    with_c_path(path, |path| {
        syscall(SYSCALL_CHDIR, [path as usize, 0, 0, 0, 0, 0])
    })
}

fn linux_mount(
    source: Option<&str>,
    target: &str,
    fs_type: Option<&str>,
    flags: usize,
    data: Option<&str>,
) -> isize {
    with_opt_c_path(source, |source| {
        with_c_path(target, |target| {
            with_opt_c_path(fs_type, |fs_type| {
                with_opt_c_path(data, |data| {
                    syscall(
                        SYSCALL_MOUNT,
                        [
                            source as usize,
                            target as usize,
                            fs_type as usize,
                            flags,
                            data as usize,
                            0,
                        ],
                    )
                })
            })
        })
    })
}

fn linux_umount(path: &str) -> isize {
    with_c_path(path, |path| {
        syscall(SYSCALL_UMOUNT2, [path as usize, 0, 0, 0, 0, 0])
    })
}

fn socket(socket_type: usize) -> isize {
    syscall(SYSCALL_SOCKET, [AF_UNIX, socket_type, 0, 0, 0, 0])
}

fn bind(fd: usize, address: &SockAddrUn) -> isize {
    syscall(
        SYSCALL_BIND,
        [
            fd,
            address as *const SockAddrUn as usize,
            core::mem::size_of::<SockAddrUn>(),
            0,
            0,
            0,
        ],
    )
}

fn listen(fd: usize) -> isize {
    syscall(SYSCALL_LISTEN, [fd, 8, 0, 0, 0, 0])
}

fn connect(fd: usize, address: &SockAddrUn) -> isize {
    syscall(
        SYSCALL_CONNECT,
        [
            fd,
            address as *const SockAddrUn as usize,
            core::mem::size_of::<SockAddrUn>(),
            0,
            0,
            0,
        ],
    )
}

fn accept(fd: usize) -> isize {
    syscall(SYSCALL_ACCEPT, [fd, 0, 0, 0, 0, 0])
}

fn sendto(fd: usize, payload: &[u8], address: &SockAddrUn) -> isize {
    syscall(
        SYSCALL_SENDTO,
        [
            fd,
            payload.as_ptr() as usize,
            payload.len(),
            0,
            address as *const SockAddrUn as usize,
            core::mem::size_of::<SockAddrUn>(),
        ],
    )
}

fn assert_socket_node(path: &str) {
    let mut stat = Statx::default();
    let result = with_c_path(path, |path| {
        syscall(
            SYSCALL_STATX,
            [
                AT_FDCWD as usize,
                path as usize,
                0,
                STATX_TYPE,
                &mut stat as *mut Statx as usize,
                0,
            ],
        )
    });
    assert_eq!(result, 0, "statx {path}");
    assert_eq!(stat.mode & S_IFMT, S_IFSOCK, "not a socket node: {path}");
}

fn exchange_stream(listener: usize, pathname: &str, byte: u8) {
    let client = socket(SOCK_STREAM);
    assert!(client >= 0);
    assert_eq!(connect(client as usize, &SockAddrUn::pathname(pathname)), 0);
    let accepted = accept(listener);
    assert!(accepted >= 0);
    assert_eq!(write(client as usize, &[byte]), 1);
    let mut received = [0u8; 1];
    assert_eq!(read(accepted as usize, &mut received), 1);
    assert_eq!(received[0], byte);
    assert_eq!(close(accepted as usize), 0);
    assert_eq!(close(client as usize), 0);
}

fn test_ext4_aliases(root: &str, source: &str, alias: &str) {
    assert_eq!(linux_chdir(source), 0);

    let old_listener = socket(SOCK_STREAM);
    assert!(old_listener >= 0);
    assert_eq!(
        bind(old_listener as usize, &SockAddrUn::pathname("stream.sock")),
        0
    );
    assert_eq!(listen(old_listener as usize), 0);

    let original = alloc::format!("{source}/stream.sock");
    let renamed = alloc::format!("{source}/renamed.sock");
    let hardlink = alloc::format!("{source}/hard.sock");
    assert_socket_node(&original);
    assert_eq!(linux_rename("stream.sock", "renamed.sock"), 0);
    assert_eq!(linux_link("renamed.sock", "hard.sock"), 0);
    assert_eq!(linux_mount(Some(source), alias, None, MS_BIND, None), 0);

    exchange_stream(
        old_listener as usize,
        &alloc::format!("{alias}/renamed.sock"),
        0x31,
    );
    exchange_stream(old_listener as usize, &hardlink, 0x32);
    let missing_client = socket(SOCK_STREAM);
    assert!(missing_client >= 0);
    assert_eq!(
        connect(missing_client as usize, &SockAddrUn::pathname(&original)),
        ENOENT
    );
    assert_eq!(close(missing_client as usize), 0);

    // Reusing an unlinked pathname creates a new inode and must not retarget
    // the still-live old listener, which remains reachable through hard.sock.
    assert_eq!(linux_unlink(&renamed, 0), 0);
    let new_listener = socket(SOCK_STREAM);
    assert!(new_listener >= 0);
    assert_eq!(
        bind(new_listener as usize, &SockAddrUn::pathname(&renamed)),
        0
    );
    assert_eq!(listen(new_listener as usize), 0);
    assert_socket_node(&renamed);
    exchange_stream(new_listener as usize, &renamed, 0x41);
    exchange_stream(old_listener as usize, &hardlink, 0x42);

    let dgram_path = alloc::format!("{source}/wrong-type.sock");
    let dgram = socket(SOCK_DGRAM);
    assert!(dgram >= 0);
    assert_eq!(bind(dgram as usize, &SockAddrUn::pathname(&dgram_path)), 0);
    let stream = socket(SOCK_STREAM);
    assert!(stream >= 0);
    assert_eq!(
        connect(stream as usize, &SockAddrUn::pathname(&dgram_path)),
        EPROTOTYPE
    );
    assert_eq!(close(stream as usize), 0);
    assert_eq!(close(dgram as usize), 0);
    assert_eq!(linux_unlink(&dgram_path, 0), 0);

    assert_eq!(linux_umount(alias), 0);
    assert_eq!(close(old_listener as usize), 0);
    let stale_client = socket(SOCK_STREAM);
    assert!(stale_client >= 0);
    assert_eq!(
        connect(stale_client as usize, &SockAddrUn::pathname(&hardlink)),
        ECONNREFUSED
    );
    assert_eq!(close(stale_client as usize), 0);
    assert_eq!(linux_unlink(&hardlink, 0), 0);

    assert_eq!(close(new_listener as usize), 0);
    let blocked_rebind = socket(SOCK_STREAM);
    assert!(blocked_rebind >= 0);
    assert_eq!(
        bind(blocked_rebind as usize, &SockAddrUn::pathname(&renamed)),
        EADDRINUSE
    );
    assert_eq!(close(blocked_rebind as usize), 0);
    assert_eq!(linux_unlink(&renamed, 0), 0);
    assert_eq!(linux_chdir(root), 0);
}

fn test_tmpfs_lifetime(mountpoint: &str) {
    assert_eq!(
        linux_mount(
            Some("tmpfs"),
            mountpoint,
            Some("tmpfs"),
            0,
            Some("size=1m,mode=755")
        ),
        0
    );
    let pathname = alloc::format!("{mountpoint}/dgram.sock");
    let address = SockAddrUn::pathname(&pathname);

    let old_server = socket(SOCK_DGRAM);
    assert!(old_server >= 0);
    assert_eq!(bind(old_server as usize, &address), 0);
    assert_socket_node(&pathname);
    let client = socket(SOCK_DGRAM);
    assert!(client >= 0);
    assert_eq!(connect(client as usize, &address), 0);

    assert_eq!(linux_unlink(&pathname, 0), 0);
    let new_server = socket(SOCK_DGRAM);
    assert!(new_server >= 0);
    assert_eq!(bind(new_server as usize, &address), 0);

    // A connected datagram socket keeps the peer selected by connect. It must
    // not follow a new inode installed at the same pathname.
    assert_eq!(write(client as usize, b"old"), 3);
    let mut received = [0u8; 3];
    assert_eq!(read(old_server as usize, &mut received), 3);
    assert_eq!(&received, b"old");
    assert_eq!(close(old_server as usize), 0);
    assert_eq!(write(client as usize, b"dead"), ECONNREFUSED);

    assert_eq!(sendto(client as usize, b"new", &address), 3);
    assert_eq!(read(new_server as usize, &mut received), 3);
    assert_eq!(&received, b"new");

    // unix_sock.path holds an mnt reference on Linux. PinnedPath provides the
    // same externally visible busy-unmount lifetime here.
    assert_eq!(linux_umount(mountpoint), EBUSY);
    assert_eq!(linux_unlink(&pathname, 0), 0);
    assert_eq!(close(new_server as usize), 0);
    assert_eq!(close(client as usize), 0);
    assert_eq!(linux_umount(mountpoint), 0);
}

fn test_readonly_mount(mountpoint: &str) {
    assert_eq!(
        linux_mount(
            Some("tmpfs"),
            mountpoint,
            Some("tmpfs"),
            MS_RDONLY,
            Some("size=1m,mode=755")
        ),
        0
    );
    let socket_file = socket(SOCK_STREAM);
    assert!(socket_file >= 0);
    assert_eq!(
        bind(
            socket_file as usize,
            &SockAddrUn::pathname(&alloc::format!("{mountpoint}/readonly.sock")),
        ),
        EROFS
    );
    assert_eq!(close(socket_file as usize), 0);
    assert_eq!(linux_umount(mountpoint), 0);
}

fn test_abstract_namespace(pid: isize) {
    let name = alloc::format!("unix-vfs-{pid}");
    let address = SockAddrUn::abstract_name(name.as_bytes());
    let first = socket(SOCK_DGRAM);
    let second = socket(SOCK_DGRAM);
    assert!(first >= 0 && second >= 0);
    assert_eq!(bind(first as usize, &address), 0);
    assert_eq!(bind(second as usize, &address), EADDRINUSE);
    assert_eq!(close(first as usize), 0);
    assert_eq!(bind(second as usize, &address), 0);
    assert_eq!(close(second as usize), 0);
}

#[unsafe(no_mangle)]
pub fn main() -> i32 {
    let pid = getpid();
    let root = alloc::format!("/tmp/unix_vfs_path_smoke_{pid}");
    let source = alloc::format!("{root}/source");
    let alias = alloc::format!("{root}/alias");
    let tmpfs = alloc::format!("{root}/tmpfs");
    let readonly = alloc::format!("{root}/readonly");

    assert_eq!(linux_mkdir(&root), 0);
    assert_eq!(linux_mkdir(&source), 0);
    assert_eq!(linux_mkdir(&alias), 0);
    assert_eq!(linux_mkdir(&tmpfs), 0);
    assert_eq!(linux_mkdir(&readonly), 0);

    test_ext4_aliases(&root, &source, &alias);
    test_tmpfs_lifetime(&tmpfs);
    test_readonly_mount(&readonly);
    test_abstract_namespace(pid);

    assert_eq!(linux_chdir("/"), 0);
    assert_eq!(linux_unlink(&source, AT_REMOVEDIR), 0);
    assert_eq!(linux_unlink(&alias, AT_REMOVEDIR), 0);
    assert_eq!(linux_unlink(&tmpfs, AT_REMOVEDIR), 0);
    assert_eq!(linux_unlink(&readonly, AT_REMOVEDIR), 0);
    assert_eq!(linux_unlink(&root, AT_REMOVEDIR), 0);

    println!("unix_vfs_path_smoke passed");
    0
}
