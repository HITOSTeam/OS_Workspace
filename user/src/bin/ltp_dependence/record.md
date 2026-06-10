# 已经验证过的测试

## 推进节奏

- 开发定位按单组推进：每次选一组语义相近的测试，通常 5 到 20 个，
  只启用这一组做 focused regression，方便定位失败原因。
- 阶段验收按组合回归：同一语义簇连续通过几组后，再把这些组一起启用
  跑一次组合回归，确认组间没有状态污染或共享语义退化。
- 通过后把组名记录在本文件；`submit_plan.rs` 里的临时启用项在结束前恢复为注释状态。

## 已验证组

    // &super::LTP_TEST_POINTS,
    //
    // 进程生命周期 / exec / wait / 线程
    // &super::FORK_TASKS,
    // &super::WAITPID_TASKS,
    // &super::WAITID_TASKS,
    // &super::CLONE_WAIT_EXIT_CORE_TASKS,
    //
    // 进程身份 / 会话 / 优先级
    // &super::PROCINFO_TASKS,
    // &super::PGRP_SESSION_TASKS,
    // &super::SETPGRP_TASKS,
    // &super::GETPRIORITY_TASKS,
    // &super::SETPRIORITY_TASKS,
    // &super::PROC_TID_TASKS,
    // verified on riscv64 musl+glibc: getpid/getppid/getuid/geteuid/
    // getgid/getegid, getpgid/getsid, uname, gettimeofday EFAULT,
    // setpgid/setsid/setpgrp, getpriority/setpriority, getpgrp, and gettid
    // cases pass. Expected TCONF: setpriority01 PRIO_USER subcase cannot add
    // an extra user in this image.
    // &super::SCHED_NICE_CORE_TASKS,
    // &super::SCHED_TC_TASKS,
    // verified on riscv64 musl+glibc: nice01-05, scheduler priority bounds,
    // sched_get/set affinity, sched_get/set attr, sched_get/set param,
    // sched_get/set scheduler, sched_rr_get_interval, sched_yield, and
    // sched_tc0-6 pass. Expected TCONF: sched_rr_get_interval03 skips the
    // libc EFAULT subcase, while the raw syscall variant covers EFAULT.
    // &super::UNRUN_SCHED_TASKS,
    // verified on riscv64 musl+glibc: proc_sched_rt01 passes after adding
    // /extra/bin/zcat so LTP can inspect /proc/config.gz and the sched_rt/
    // sched_rr procfs knobs; starvation passes with bounded profile
    // `starvation -l 50000 -t 60`. Expected TCONF: autogroup01 because
    // sched_autogroup is unsupported.
    //
    // 凭证 / capability / key
    // &super::GETRES_TASKS,
    // &super::CRED_SET_CORE_TASKS,
    // &super::CRED_SET_RES_TASKS,
    // &super::CRED_FS_TASKS,
    // &super::CRED_EGID_TASKS,
    // verified on riscv64 musl+glibc with no FAIL/TBROK/TCONF:
    // getresuid/getresgid, setuid/setgid, setreuid/setregid,
    // setresuid/setresgid, setfsuid/setfsgid, and setegid cases pass,
    // including saved-id restore, EPERM, EFAULT, and file-permission probes.
    // &super::CRED_KEY_CAP_CORE_TASKS,
    // verified on riscv64 musl+glibc with no FAIL/TBROK: acct02, acl1,
    // capget01-02, and capset01-02 pass. Expected TCONF: add_key01-04
    // because keyring syscalls are unsupported on this arch/image, add_key05
    // lacks useradd, and check_keepcaps needs linux/securebits.h or libcap.
    // &super::CAP_CRED16_QUERY_TASKS,
    // &super::CRED16_MUTATION_TASKS,
    // &super::CRED_SETGROUPS_GLIBC_ONLY_TASKS,
    // verified on riscv64 with no FAIL/TBROK: capset03-04,
    // getegid01-02_16, getgroups03, setfsgid03, setgroups04, and
    // glibc-only setgroups03 pass.
    // Expected TCONF: most *_16 compat syscalls are unsupported on riscv64,
    // and capability-bound tests needing sys/capability.h are unavailable.
    // check_simple_capset is separated into CRED_CAP_LIBCAP_BUILD_GAP_TASKS:
    // when LTP is built without libcap, its source returns 1 instead of
    // reporting TCONF, so it is not part of the normal kernel regression.
    // setgroups03 is glibc-only because default musl exposes NGROUPS=32 in
    // this test, while modern Linux setgroups(2) uses NGROUPS_MAX=65536.
    //
    // 信号 / futex / eventfd / timerfd / epoll
    // &super::SIGACTION_SIGNAL_CORE_TASKS,
    // &super::KILL_PAUSE_TGKILL_TASKS,
    // &super::CLOCK_TIMERFD_SIGNALFD_TASKS,
    // &super::EVENTFD_FUTEX_TIMERFD_TASKS,
    // &super::EPOLL_CORE_TASKS,
    // &super::SIGNAL_GLIBC_ONLY_TASKS,
    // &super::EPOLL_GLIBC_ONLY_TASKS,
    // verified on riscv64 musl+glibc as one combined batch for the five
    // default groups; SIGNAL_GLIBC_ONLY_TASKS and EPOLL_GLIBC_ONLY_TASKS also
    // pass in the glibc lane. Validation command: ARCH=riscv64 bash os/run.sh.
    // Fixed during this batch: kill11 WCOREDUMP status now follows Linux
    // core-dump signals and RLIMIT_CORE; timerfd_settime02 no longer thrashes
    // the global timerfd schedule on repeated disarm/CANCEL_ON_SET toggles.
    // Expected TCONF/skip: signal06 x86_64-only, old sigpending/
    // rt_sigqueueinfo syscalls unavailable on riscv64, kill13 kernel config,
    // eventfd/libaio config probes, futex hugetlb/waitv version probes,
    // timerfd04 time namespace config, and old epoll_create syscall probes.
    // Default musl signal-wait and epoll_create(size) wrappers do not preserve
    // the same kernel ABI probes as glibc; optional adapters remain disabled.
    // timerfd_settime02 is a long runtime-style race test; in the combined run
    // both musl and glibc variants exited with "Nothing bad happened" and pass.
    //
    // 基础文件 I/O / fd / fcntl
    // &super::CWD_DIR_TASKS,
    // &super::ACCESS_TASKS,
    // &super::FACCESSAT_TASKS,
    // &super::CLOSE_TASKS,
    // &super::OPEN_CORE_TASKS,
    // &super::OPEN_EXT_TASKS,
    // &super::OPENAT_CORE_TASKS,
    // &super::CLOSE_RANGE_CORE_TASKS,
    // &super::UMASK_TASKS,
    // verified on riscv64 musl+glibc: getcwd01-04, chdir01/04,
    // access01-04, faccessat01-02, faccessat201-202, close01-02,
    // open01-04/06-11/13, openat01/03, close_range02, and umask01 pass.
    // faccessat2 syscall 439 is implemented for AT_EACCESS,
    // AT_SYMLINK_NOFOLLOW, and AT_EMPTY_PATH coverage.
    // &super::DUP_CORE_TASKS,
    // &super::DUP_FCNTL_TASKS,
    // &super::FCNTL_BASIC_TASKS,
    // &super::FCNTL_EXTENDED_TASKS,
    // &super::FCNTL_MISC_TASKS,
    // &super::FCNTL_LEASE_TASKS,
    // verified on riscv64 musl+glibc: dup01-07, dup3_01-02,
    // dup201-207, fcntl01-05/07-27/29-37 with 64-bit variants,
    // llseek01-03, getdents64 paths in getdents01-02, and unlink05 pass.
    // Expected TCONF/skip: fcntl38-39 require CONFIG_DNOTIFY; old
    // SYS_getdents/libc getdents are unavailable on riscv64/glibc.
    // &super::READ_WRITE_LSEEK_TASKS,
    // verified on riscv64 musl+glibc: read01-04, readv01-02,
    // write02-06, writev01-03/05-07, and lseek01-02/07 pass.
    // &super::PREAD_PWRITE_PREADV_TASKS,
    // &super::PREADV2_PWRITEV2_TASKS,
    // verified on riscv64 musl+glibc: close_range01, pread01-02,
    // pwrite01-04, preadv01-03, pwritev01-02, preadv201-202,
    // and pwritev201-202 pass, including 64-bit variants.
    //
    // 文件元数据 / 目录树 / 链接 / xattr
    // &super::CHOWN_TASKS,
    // &super::CHMOD_TASKS,
    // &super::FCHMOD_TASKS,
    // &super::FCHOWN_TASKS,
    // &super::FCHDIR_TASKS,
    // &super::CREAT_CORE_TASKS,
    // &super::FSTAT_TASKS,
    // &super::FSTATFS_TASKS,
    // &super::STATFS_TASKS,
    // &super::STATX_BASIC_TASKS,
    // &super::STAT_TASKS,
    // &super::STAT_LFS_EXT_TASKS,
    // &super::STATX_EXT_TASKS,
    // &super::MKNODAT_TASKS,
    // &super::MKNOD_CORE_TASKS,
    // &super::MKDIR_CORE_TASKS,
    // &super::RMDIR_TASKS,
    // &super::LINK_CORE_TASKS,
    // &super::SYMLINK_CORE_TASKS,
    // &super::READLINK_CORE_TASKS,
    // &super::COPY_TRUNCATE_CORE_TASKS,
    // &super::CREAT_USERFAULTFD_TASKS,
    // &super::RENAME_UNLINK_TASKS,
    // &super::XATTR_CORE_TASKS,
    // &super::FS_META_CHOWN_XATTR_TASKS,
    // &super::FS_META_INOTIFY_XATTR_TASKS,
    // verified on riscv64 musl+glibc: chown01-05 and
    // chmod01/03/05/06/07, fchmod01-06, fchmodat01-02,
    // fchown01-05, fchownat01-02, fchdir01-03,
    // creat01/03/04/05/06/08, fstat02-03, fstatat01,
    // fstat02-03 64-bit variants, fstatfs01 and fstatfs01-02 64-bit
    // variants, statfs01 and statfs01-03 64-bit variants, lstat01-02
    // with 64-bit variants, stat01-03 with 64-bit variants,
    // statx01-05/06-12, mknodat01,
    // mknod01-06/08/09, mkdir02-05/09, mkdirat01-02, rmdir01-03,
    // link02/04/05/08, linkat01-02, symlink01-04, symlinkat01,
    // readlink01/03, readlinkat01-02, copy_file_range01-03,
    // ftruncate01/03/04 with 64-bit variants, truncate02 with 64-bit
    // variant, truncate03, rename01/03-14, renameat01/201/202,
    // unlink07-09, and unlinkat01 pass.
    // Expected statx01 TCONF: stx_mnt_id is not present/supported by
    // this LTP/libc combination, while the syscall behavior under test passes.
    // Expected statx06-12 TCONF coverage: mkfs.ext4/exportfs tools,
    // CONFIG_FS_VERITY, ext4/xfs-only checks, STATX_DIOALIGN, and
    // STATX_ATTR_MOUNT_ROOT are unavailable in this image; statx08 still
    // validates append/immutable/nodump attributes and only skips compression.
    // Expected statx05 TCONF: mkfs.ext4 is unavailable in this image.
    // CREAT_USERFAULTFD_TASKS is verified on riscv64 musl+glibc:
    // creat07, creat09, and userfaultfd01 pass.
    // Expected linkat02 TCONF: the hardlink-limit EMLINK probe is not
    // appropriate in this image, while the other linkat errno cases pass.
    // Expected rename11/renameat01 TCONF: the EMLINK limit probe is not
    // appropriate in this image, while the remaining rename errno cases pass.
    // Expected copy_file_range02 TCONF: chattr/swapfile/loopdev probes are
    // unavailable in this image, while the remaining errno cases pass.
    // Expected ftruncate04 TCONF: CONFIG_MANDATORY_FILE_LOCKING is absent.
    // XATTR_CORE_TASKS was run on riscv64 musl+glibc: getxattr01-05,
    // setxattr01-03, listxattr01-03, removexattr01-02, lgetxattr01-02,
    // pselect02_64, pselect03, pselect03_64, and epoll_pwait05 pass;
    // getxattr04/getxattr05 only report expected environment TCONF.
    // epoll-ltp passes on glibc but reports a known default-musl runtime
    // TFAIL because epoll_create(size) does not check size <= 0 before
    // calling epoll_create1(0) on this arch. Optional libltp_epoll_create_fix.so
    // remains available but disabled by default.
    // fchmodat/fchownat path chmod/chown now resolves the inode before
    // readonly-mount checks and avoids ext4_lock self-deadlock in rofs lookup.
    // Directory mutation paths precompute readonly-mount state before ext4_lock
    // to avoid re-entering path/fd resolution while holding the filesystem lock.
    // link/linkat compare logical mount identities for Linux old_path.mnt !=
    // new_path.mnt EXDEV semantics, not only the backing device id.
    // FS_META_INOTIFY_XATTR_TASKS was run on riscv64 musl+glibc with no
    // FAIL/TBROK: lchown01-03 and fgetxattr/fsetxattr/flistxattr/
    // fremovexattr/llistxattr/lremovexattr cases pass. Expected TCONF:
    // 16-bit chown/fchown/lchown compat syscalls are unsupported on this
    // platform, inotify01-12 and inotify_init1_01/02 require inotify syscalls
    // that are not implemented yet, inotify07-08 also require overlayfs, and
    // fsetxattr02 requires the brd driver.
    //
    // 高级文件 I/O / sync / fallocate
    // &super::FALLOCATE_FSYNC_SYNC_TASKS,
    // &super::PIPE_CORE_TASKS,
    // &super::PIPE_SENDFILE_SPLICE_TASKS,
    // &super::TEE_VMSPLICE_FADVISE_TASKS,
    // &super::AIO_DIO_CORE_TASKS,
    // &super::IOCTL_IOURING_OPEN_TASKS,
    // verified on riscv64 musl+glibc: fallocate01-06, fdatasync01-03,
    // fsync01-04, sync01, sync_file_range01-02, syncfs01, truncate03_64,
    // readdir01, readdir21, pipe01-15, pipe2_01/02/04, sendfile02-06
    // with 64-bit variants, splice01-09, sendfile07-09 with 64-bit
    // variants, tee01-02, vmsplice01-04, posix_fadvise01-04, dio_append,
    // dio_read, diotest1-6, dma_thread_diotest -w 2, ioctl01-02,
    // ioctl04-07, open12, open14, openat02, and openat04 pass.
    // dio_sparse -s 1M -n 2 and dio_truncate -n 2 -a 2 -c 2 pass as
    // bounded stress checks; their default 100M+/16-reader forms are too
    // slow in current QEMU/ext4 path and were interrupted without observed
    // FAIL/TBROK.
    // Expected TCONF: fallocate04 requires FALLOC_FL_PUNCH_HOLE,
    // fallocate05 probes unsupported fallocate mode, fallocate06 needs a
    // larger tmpfs budget, and readdir21 targets old __NR_readdir which is
    // unavailable on riscv64. splice05 reports TCONF for AF_UNIX sockets
    // because this image/kernel path only supports splice with pipes/files;
    // sendfile09 needs 5G free space; splice06 needs
    // /proc/sys/kernel/domainname; splice07 skips optional fd providers such
    // as fanotify/inotify/io_uring/memfd_secret; splice08-09 require Linux
    // 6.7+ behavior. AIO/libaio cases report TCONF due missing libaio or
    // CONFIG_AIO-style config probes; readahead01 is unavailable on riscv64
    // in this image, readahead02 requires /proc/self/io. doio still needs
    // its iogen pipe adapter before re-entering an active batch. ioctl03
    // needs TUN, ioctl08 needs btrfs, ioctl09 needs parted, ioctl_loop01-07
    // need loop devices, ioctl_sg01 needs a usable SCSI device, io_uring01-02
    // need CONFIG_IO_URING, openat201-203 need openat2, and openat02
    // O_NOATIME probes may TCONF on unsupported mounts.
    // fsync02 previously exposed ext4-fs sparse-write EOPNOTSUPP when the
    // file needed more than four extent leaf blocks; ext4 extent writes now
    // support depth-2 index trees and free old metadata blocks when
    // rewriting/truncating the tree.
    // openat04 exposed linkat(/proc/self/fd/N, ..., AT_SYMLINK_FOLLOW)
    // returning EXDEV before resolving the proc-fd magic link; linkat now
    // applies the cross-mount check to the resolved inode, matching Linux.
    // &super::UNRUN_FS_TASKS,
    // verified on riscv64 musl+glibc: fs_di, ftest01-08, inode01-02,
    // and stream01-05 pass. fs_di is registered as `fs_di -d /fs_di_ltp`
    // because submit_plan entries are whitespace-split without shell
    // variable expansion, and /tmp/tmpfs is too small for its 30MiB datafile.
    // Expected TCONF: fs_fill reports no enough memory for tmpfs use, and
    // squashfs01 reports missing mksquashfs in this image.
    // fs_di still prints non-fatal coreutils chmod -R EBADF/fts_read warnings
    // on deep random directory trees; the data integrity checks pass and this
    // should be cleaned up in a later chmod/fts metadata pass.
    //
    // mount / namespace / fanotify / proc-sysfs
    // &super::NS_MOUNT_CORE_TASKS,
    // &super::MOUNT_API_TASKS,
    // &super::NS_MOUNT_FOLLOWUP_TASKS,
    // &super::PIDNS_MODULE_TASKS,
    // &super::FANOTIFY_CORE_TASKS,
    // verified on riscv64 musl+glibc with no FAIL/TBROK: setns01,
    // setns02, unshare01, and unshare02 pass. Expected TCONF: mountns01-04,
    // timens01, and pidns01 require unsupported namespace/kernel config;
    // setns02 skips CLONE_NEWUTS while still validating the IPC namespace path.
    // MOUNT_API_TASKS was run on riscv64: fanotify21-23 are expected TCONF
    // for missing fanotify/debugfs/mkfs.ext2 support; mount01-07, umount01-03,
    // umount2_01-02, mount_setattr01, fsconfig01-03, and fsopen01 pass on
    // glibc. mount02 exposed ordinary mount-to-existing-mountpoint wrongly
    // succeeding; new mounts now return EBUSY when the target is already a
    // mountpoint, matching Linux. mount07 still fails on default musl because
    // musl realpath(3) follows the nosymfollow path differently; the same test
    // passes on glibc and kernel open/readlink/statfs checks pass.
    // NS_MOUNT_FOLLOWUP_TASKS was run on riscv64 musl+glibc with no
    // FAIL/TBROK: fsopen02, fsmount01-02, fspick01-02, open_tree01-02,
    // move_mount01-02, and pidns04 pass. Expected TCONF: userns01/06 need
    // libcap, and userns02-05/07-08 plus pidns02-03 require unsupported
    // namespace/kernel config in this image.
    // PIDNS_MODULE_TASKS was run on riscv64 musl+glibc with no FAIL/TBROK:
    // pidns05-06/10/13/16-17/30-32 and getcpu01 pass. Expected TCONF:
    // pidns12/20 need unsupported namespace/kernel config, module tests lack
    // test .ko files or arch support, and membarrier01 is unsupported here.
    // FANOTIFY_CORE_TASKS was run on riscv64 with no FAIL/TBROK; fanotify01-20
    // all report expected TCONF because fanotify is not configured in this
    // kernel, with fanotify13 additionally skipping overlayfs-on-tmpfs cases.
    // &super::UNAME_SYSFS_ASLR_TASKS,
    // verified on riscv64 musl+glibc with no FAIL/TBROK: newuname01,
    // utsname01-04, sysconf01, getpagesize01, and syscall01 pass. Expected
    // TCONF: cve-2017-2618 needs /proc/self/attr/fscreate, cve-2017-2671
    // needs IPPROTO_ICMP sockets, cve-2022-4378/aslr01 require unsupported
    // kernel config, old sysfs/_sysctl syscalls are unavailable on riscv64,
    // and some libc sysconf resources are unsupported.
    // &super::IO_PERF_SYSINFO_PATHCONF_TASKS,
    // verified on riscv64: ioprio_get01, ioprio_set01-03, sysinfo01-02,
    // personality01-02, confstr01, pathconf01-02, and fpathconf01 pass on
    // glibc; arch/misc config-only cases report expected TCONF where
    // unsupported. pathconf02 fails only on default musl because musl
    // pathconf(_PC_LINK_MAX) returns a static limit for several bad paths
    // instead of validating the path and returning ENOTDIR/ENOENT/EACCES/ELOOP.
    // Expected TCONF: ioperm/iopl are x86-only, perf_event is unsupported,
    // sysinfo03 needs unsupported kernel config, and ioprio_set01 skips the
    // optional priority-decrease probe.
    // &super::KCMP_TASKS,
    // verified on riscv64 musl+glibc with no FAIL/TBROK: kcmp01 reports
    // expected TCONF because __NR_kcmp is unsupported on this arch/image.
    // &super::UTS_NAME_TASKS,
    // &super::UTS_QUERY_TASKS,
    // &super::GETRANDOM_TASKS,
    // verified on riscv64 musl+glibc with no FAIL/TBROK: sethostname01-03,
    // setdomainname01-03, gethostname01, getdomainname01, and getrandom01-05
    // pass, including setter success paths, EINVAL/EFAULT/EPERM probes, and
    // getrandom invalid-buffer/invalid-flag checks.
    //
    // IPC / POSIX MQ / SysV IPC
    // &super::SYSV_SHM_CORE_TASKS,
    // verified on riscv64 musl+glibc for shmat02-04, shmctl01-08,
    // shmdt01-02, shmget03-06, and shmt02. Expected LTP TCONF/skip:
    // shmctl02 libc EFAULT variant skip, shmctl04 SHM_STAT_ANY, shmctl05
    // remap_file_pages, shmctl06 shmid64 time_high, shmget05-06
    // CONFIG_CHECKPOINT_RESTORE. shmat1 is left for separate
    // scheduler/runtime investigation because this old pthread stress case
    // hangs near the tail of its unsynchronized done_shmat handoff.
    // &super::SYSV_SHM_FOLLOWUP_TASKS,
    // verified on riscv64: glibc passes shmget02 and shmt03-10; musl passes
    // shmget02, shmt03-08, and shmt10. musl shmt09 fails at the first sbrk()
    // without entering the kernel brk syscall, so it is a libc/runtime wrapper
    // limitation; optional libltp_sbrk_fix.so remains available but disabled.
    // &super::SYSV_IPC_CORE_TASKS,
    // verified on riscv64 musl+glibc: msgctl01-02, msgget01-02, msgrcv01,
    // msgsnd01, semctl01-02, semop01-02, semget01, and shmat01 pass.
    // semop02 has expected TCONF lines for semtimedop-only cases under the
    // plain semop variant.
    // &super::SYSV_IPC_EXT_TASKS,
    // verified on riscv64 musl+glibc: msgctl03-06, msgget03-04,
    // msgrcv02-03, msgsnd02/05, semctl03-08, semop03-05, and semget05 pass.
    // Expected TCONF/skip: msgctl04 and semctl03 libc EFAULT variants,
    // msgctl05/semctl08 time_high fields, and msgget04/msgrcv03
    // CONFIG_CHECKPOINT_RESTORE.
    // &super::SYSV_MSG_STRESS_TASKS,
    // not marked verified yet: msgstress01 functionally reports TPASS and all
    // messages are received on riscv64 musl+glibc, but both variants emit TWARN
    // "Out of runtime during forking" and return 4 under the current harness.
    // Treat this as stress/runtime scale follow-up, not as a message queue
    // correctness failure.
    // &super::IPC_NAMESPACE_TASKS,
    // verified on riscv64 musl+glibc: msg_comm, sem_comm, shm_comm,
    // shmem_2nstest, shmnstest, mesgq_nstest, sem_nstest, and semtest_2ns
    // pass. mqns_01-04 are expected CONFIG_USER_NS TCONF in this image.
    // &super::POSIX_MQ_SYSV_MSG_SEM_TASKS,
    // verified on riscv64 musl+glibc: POSIX MQ cases, msgctl12, msgrcv05-08,
    // msgsnd06, semctl09, and semget02 pass. Expected TCONF/skip: 16-bit
    // setuid/setreuid compat cases are unsupported on this platform; msgget05
    // requires CONFIG_CHECKPOINT_RESTORE.
