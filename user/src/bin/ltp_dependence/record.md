# 已经验证过的测试

## 推进节奏

- 开发定位按单组推进：每次选一组语义相近的测试，通常 5 到 20 个，
  只启用这一组做专项回归，方便定位失败原因。
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
    // 已在 riscv64 musl+glibc 验证：getpid/getppid/getuid/geteuid/
    // getgid/getegid、getpgid/getsid、uname、gettimeofday EFAULT、
    // setpgid/setsid/setpgrp、getpriority/setpriority、getpgrp、gettid
    // 相关用例通过。预期 TCONF：setpriority01 的 PRIO_USER 子项在当前
    // 镜像中无法额外创建用户。
    // &super::SCHED_NICE_CORE_TASKS,
    // &super::SCHED_TC_TASKS,
    // 已在 riscv64 musl+glibc 验证：nice01-05、调度优先级边界、
    // sched_get/set affinity、sched_get/set attr、sched_get/set param、
    // sched_get/set scheduler、sched_rr_get_interval、sched_yield、
    // sched_tc0-6 通过。预期 TCONF：sched_rr_get_interval03 跳过 libc
    // EFAULT 子项，raw syscall variant 已覆盖 EFAULT。
    // &super::UNRUN_SCHED_TASKS,
    // 已在 riscv64 musl+glibc 验证：加入 /extra/bin/zcat 后，LTP 可以检查
    // /proc/config.gz 以及 sched_rt/sched_rr procfs 开关，proc_sched_rt01
    // 通过；starvation 使用受限参数 `starvation -l 50000 -t 60` 通过。
    // 预期 TCONF：autogroup01，因为当前不支持 sched_autogroup。
    //
    // 凭证 / capability / key
    // &super::GETRES_TASKS,
    // &super::CRED_SET_CORE_TASKS,
    // &super::CRED_SET_RES_TASKS,
    // &super::CRED_FS_TASKS,
    // &super::CRED_EGID_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK/TCONF：
    // getresuid/getresgid、setuid/setgid、setreuid/setregid、
    // setresuid/setresgid、setfsuid/setfsgid、setegid 相关用例通过，
    // 覆盖 saved-id 恢复、EPERM、EFAULT 和文件权限探针。
    // &super::CRED_KEY_CAP_CORE_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK：acct02、acl1、
    // capget01-02、capset01-02 通过。预期 TCONF：add_key01-04 因当前
    // 架构/镜像不支持 keyring syscalls；add_key05 缺少 useradd；
    // check_keepcaps 需要 linux/securebits.h 或 libcap。
    // &super::CAP_CRED16_QUERY_TASKS,
    // &super::CRED16_MUTATION_TASKS,
    // &super::CRED_SETGROUPS_GLIBC_ONLY_TASKS,
    // 已在 riscv64 验证且无 FAIL/TBROK：capset03-04、getegid01-02_16、
    // getgroups03、setfsgid03、setgroups04、glibc-only setgroups03 通过。
    // 预期 TCONF：多数 *_16 compat syscalls 在 riscv64 不支持，需要
    // sys/capability.h 的 capability-bound 测试不可用。
    // check_simple_capset 被单独放入 CRED_CAP_LIBCAP_BUILD_GAP_TASKS：
    // LTP 未启用 libcap 构建时，该源码返回 1 而不是报告 TCONF，因此不纳入
    // 常规内核回归。setgroups03 只跑 glibc，因为默认 musl 在该测试中暴露
    // NGROUPS=32，而现代 Linux setgroups(2) 使用 NGROUPS_MAX=65536。
    //
    // 信号 / futex / eventfd / timerfd / epoll
    // &super::SIGACTION_SIGNAL_CORE_TASKS,
    // &super::KILL_PAUSE_TGKILL_TASKS,
    // &super::CLOCK_TIMERFD_SIGNALFD_TASKS,
    // &super::EVENTFD_FUTEX_TIMERFD_TASKS,
    // &super::EPOLL_CORE_TASKS,
    // &super::SIGNAL_GLIBC_ONLY_TASKS,
    // &super::EPOLL_GLIBC_ONLY_TASKS,
    // 五个默认组已在 riscv64 musl+glibc 组合批次中验证；
    // SIGNAL_GLIBC_ONLY_TASKS 与 EPOLL_GLIBC_ONLY_TASKS 在 glibc lane 也通过。
    // 验证命令：ARCH=riscv64 bash os/run.sh。
    // 本批修复：kill11 的 WCOREDUMP 状态现在遵循 Linux core-dump signal
    // 与 RLIMIT_CORE 语义；timerfd_settime02 反复 disarm/CANCEL_ON_SET 切换时
    // 不再反复冲击全局 timerfd schedule。
    // 预期 TCONF/skip：signal06 仅支持 x86_64；旧 sigpending/
    // rt_sigqueueinfo syscalls 在 riscv64 不可用；kill13 依赖内核配置；
    // eventfd/libaio 配置探针、futex hugetlb/waitv 版本探针、timerfd04 时间
    // namespace 配置、旧 epoll_create syscall 探针均跳过。
    // 默认 musl 的 signal-wait 与 epoll_create(size) wrapper 不保留 glibc
    // 相同的 kernel ABI 探针行为；可选 adapter 保持禁用。timerfd_settime02
    // 是长时间 runtime 风格竞态测试，组合运行中 musl/glibc 两个 variant 都以
    // "Nothing bad happened" 退出并通过。
    //
    // 时间 / 资源 / 系统信息
    // &super::GETITIMER_TASKS,
    // &super::SETITIMER_TASKS,
    // &super::GETRUSAGE_TASKS,
    // &super::CLOCK_GETTIME_TASKS,
    // &super::CLOCK_SETTIME_TASKS,
    // &super::CLOCK_RES_TASKS,
    // &super::CLOCK_NANOSLEEP_TASKS,
    // &super::TIME_MISC_TASKS,
    // &super::ALARM_TASKS,
    // &super::POSIX_TIMER_TASKS,
    // &super::ADJTIMEX_SETTIMEOFDAY_UTIME_TASKS,
    // &super::UTIME_UNAME_ARCH_PRCTL_TASKS,
    // &super::SETRLIMIT_TASKS,
    // &super::GETRLIMIT_TASKS,
    // &super::ROBUST_TID_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK：
    // getitimer/setitimer、getrusage、clock_gettime/settime/getres/
    // nanosleep、gettimeofday/times/nanosleep、alarm、POSIX timers、
    // adjtimex/clock_adjtime/settimeofday/utime/utimes/leapsec、
    // utimensat/uname/arch_prctl、setrlimit/getrlimit、robust-list/
    // set_tid_address 相关用例通过。本批修复：utimensat 现在遵循 Linux
    // EFAULT/EPERM/UTIME_OMIT 顺序；wait 回收时累计 zombie 子进程 CPU 时间，
    // 因此 times03 在 waitpid() 后能看到非零 tms_cutime/tms_cstime。
    // 预期 TCONF/skip：getrusage02、adjtimex02、clock_nanosleep01 的 libc
    // wrapper EFAULT 探针；clock_gettime03/clock_nanosleep03 的 CONFIG_TIME_NS
    // 检查；clock_getres 和 POSIX timer 中不支持的 alarm clock id
    // CLOCK_BOOTTIME*、CLOCK_REALTIME_ALARM、CLOCK_TAI；riscv64 当前无
    // futimesat/stime syscall；arch_prctl01 是 x86 专项。
    //
    // cgroup / controller
    // &super::UNRUN_CONTROLLERS_TASKS,
    // verified on riscv64 musl+glibc: memcg_test_3 passes. Fixed during
    // this batch: syscall-heavy parent loops now hit a syscall-return
    // cooperative preemption point, so the forked signal sender is not
    // starved before the harness timeout; cgroup mkdir/rmdir also takes the
    // cgroup pseudo-fs path directly and rejects reserved control-file names.
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
    // 已在 riscv64 musl+glibc 验证：getcwd01-04、chdir01/04、
    // access01-04、faccessat01-02、faccessat201-202、close01-02、
    // open01-04/06-11/13、openat01/03、close_range02、umask01 通过。
    // faccessat2 syscall 439 已实现，覆盖 AT_EACCESS、AT_SYMLINK_NOFOLLOW
    // 与 AT_EMPTY_PATH。
    // &super::DUP_CORE_TASKS,
    // &super::DUP_FCNTL_TASKS,
    // &super::FCNTL_BASIC_TASKS,
    // &super::FCNTL_EXTENDED_TASKS,
    // &super::FCNTL_MISC_TASKS,
    // &super::FCNTL_LEASE_TASKS,
    // 已在 riscv64 musl+glibc 验证：dup01-07、dup3_01-02、dup201-207、
    // fcntl01-05/07-27/29-37 及其 64-bit variant、llseek01-03、
    // getdents01-02 中的 getdents64 路径、unlink05 通过。
    // 预期 TCONF/skip：fcntl38-39 需要 CONFIG_DNOTIFY；旧 SYS_getdents/
    // libc getdents 在 riscv64/glibc 不可用。
    // &super::READ_WRITE_LSEEK_TASKS,
    // 已在 riscv64 musl+glibc 验证：read01-04、readv01-02、write02-06、
    // writev01-03/05-07、lseek01-02/07 通过。
    // &super::PREAD_PWRITE_PREADV_TASKS,
    // &super::PREADV2_PWRITEV2_TASKS,
    // 已在 riscv64 musl+glibc 验证：close_range01、pread01-02、
    // pwrite01-04、preadv01-03、pwritev01-02、preadv201-202、
    // pwritev201-202 通过，包括 64-bit variant。
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
    // 已在 riscv64 musl+glibc 验证：chown01-05、chmod01/03/05/06/07、
    // fchmod01-06、fchmodat01-02、fchown01-05、fchownat01-02、
    // fchdir01-03、creat01/03/04/05/06/08、fstat02-03、fstatat01、
    // fstat02-03 64-bit variant、fstatfs01 与 fstatfs01-02 64-bit variant、
    // statfs01 与 statfs01-03 64-bit variant、lstat01-02 及 64-bit variant、
    // stat01-03 及 64-bit variant、statx01-05/06-12、mknodat01、
    // mknod01-06/08/09、mkdir02-05/09、mkdirat01-02、rmdir01-03、
    // link02/04/05/08、linkat01-02、symlink01-04、symlinkat01、
    // readlink01/03、readlinkat01-02、copy_file_range01-03、
    // ftruncate01/03/04 及 64-bit variant、truncate02 及 64-bit variant、
    // truncate03、rename01/03-14、renameat01/201/202、unlink07-09、
    // unlinkat01 通过。
    // statx01 预期 TCONF：该 LTP/libc 组合没有 stx_mnt_id 或不支持它，
    // 但被测 syscall 行为通过。statx06-12 预期 TCONF 覆盖：当前镜像缺少
    // mkfs.ext4/exportfs 工具、CONFIG_FS_VERITY、ext4/xfs-only 检查、
    // STATX_DIOALIGN、STATX_ATTR_MOUNT_ROOT；statx08 仍验证 append/
    // immutable/nodump 属性，只跳过 compression。statx05 预期 TCONF：
    // 当前镜像缺少 mkfs.ext4。
    // CREAT_USERFAULTFD_TASKS 已在 riscv64 musl+glibc 验证：creat07、
    // creat09、userfaultfd01 通过。
    // linkat02 预期 TCONF：hardlink-limit EMLINK 探针不适合当前镜像，
    // 其他 linkat errno 用例通过。rename11/renameat01 预期 TCONF：
    // EMLINK limit 探针不适合当前镜像，其余 rename errno 用例通过。
    // copy_file_range02 预期 TCONF：当前镜像缺少 chattr/swapfile/loopdev
    // 探针，其余 errno 用例通过。ftruncate04 预期 TCONF：
    // 缺少 CONFIG_MANDATORY_FILE_LOCKING。
    // XATTR_CORE_TASKS 已在 riscv64 musl+glibc 运行：getxattr01-05、
    // setxattr01-03、listxattr01-03、removexattr01-02、lgetxattr01-02、
    // pselect02_64、pselect03、pselect03_64、epoll_pwait05 通过；
    // getxattr04/getxattr05 只报告预期环境 TCONF。
    // epoll-ltp 在 glibc 通过，但默认 musl runtime 有已知 TFAIL：该架构上
    // epoll_create(size) 在调用 epoll_create1(0) 前没有检查 size <= 0。
    // 可选 libltp_epoll_create_fix.so 保持可用但默认禁用。
    // fchmodat/fchownat 的路径 chmod/chown 现在先解析 inode，再做
    // readonly-mount 检查，并避免 rofs lookup 中 ext4_lock 自死锁。
    // 目录修改路径在 ext4_lock 之前预计算 readonly-mount 状态，避免持有
    // 文件系统锁时重新进入 path/fd 解析。link/linkat 为匹配 Linux
    // old_path.mnt != new_path.mnt 的 EXDEV 语义，比较逻辑 mount identity，
    // 不只比较 backing device id。
    // FS_META_INOTIFY_XATTR_TASKS 已在 riscv64 musl+glibc 运行且无
    // FAIL/TBROK：lchown01-03 与 fgetxattr/fsetxattr/flistxattr/
    // fremovexattr/llistxattr/lremovexattr 相关用例通过。预期 TCONF：
    // 16-bit chown/fchown/lchown compat syscalls 在该平台不支持；
    // inotify01-12 与 inotify_init1_01/02 需要尚未实现的 inotify syscalls；
    // inotify07-08 还需要 overlayfs；fsetxattr02 需要 brd driver。
    //
    // 高级文件 I/O / sync / fallocate
    // &super::FALLOCATE_FSYNC_SYNC_TASKS,
    // &super::PIPE_CORE_TASKS,
    // &super::PIPE_SENDFILE_SPLICE_TASKS,
    // &super::TEE_VMSPLICE_FADVISE_TASKS,
    // &super::AIO_DIO_CORE_TASKS,
    // &super::IOCTL_IOURING_OPEN_TASKS,
    // 已在 riscv64 musl+glibc 验证：fallocate01-06、fdatasync01-03、
    // fsync01-04、sync01、sync_file_range01-02、syncfs01、truncate03_64、
    // readdir01、readdir21、pipe01-15、pipe2_01/02/04、sendfile02-06
    // 及 64-bit variant、splice01-09、sendfile07-09 及 64-bit variant、
    // tee01-02、vmsplice01-04、posix_fadvise01-04、dio_append、dio_read、
    // diotest1-6、dma_thread_diotest -w 2、ioctl01-02、ioctl04-07、
    // open12、open14、openat02、openat04 通过。
    // dio_sparse -s 1M -n 2 与 dio_truncate -n 2 -a 2 -c 2 作为受限压测
    // 通过；默认 100M+/16-reader 形式在当前 QEMU/ext4 路径太慢，曾被中断，
    // 但未观察到 FAIL/TBROK。
    // 预期 TCONF：fallocate04 需要 FALLOC_FL_PUNCH_HOLE；fallocate05 探测
    // 不支持的 fallocate mode；fallocate06 需要更大的 tmpfs 预算；
    // readdir21 目标是 riscv64 不可用的旧 __NR_readdir。splice05 因 AF_UNIX
    // socket 报 TCONF，因为当前镜像/内核路径只支持 pipe/file 的 splice；
    // sendfile09 需要 5G 可用空间；splice06 需要 /proc/sys/kernel/domainname；
    // splice07 跳过 fanotify/inotify/io_uring/memfd_secret 等可选 fd provider；
    // splice08-09 需要 Linux 6.7+ 行为。AIO/libaio 用例因缺少 libaio 或
    // CONFIG_AIO 类配置探针而 TCONF；readahead01 在当前 riscv64 镜像不可用，
    // readahead02 需要 /proc/self/io。doio 重新进入 active batch 前仍需要
    // iogen pipe adapter。ioctl03 需要 TUN，ioctl08 需要 btrfs，ioctl09
    // 需要 parted，ioctl_loop01-07 需要 loop device，ioctl_sg01 需要可用
    // SCSI device，io_uring01-02 需要 CONFIG_IO_URING，openat201-203 需要
    // openat2，openat02 的 O_NOATIME 探针在不支持的 mount 上可 TCONF。
    // fsync02 之前暴露 ext4-fs sparse-write 在文件需要超过四个 extent leaf
    // block 时返回 EOPNOTSUPP；现在 ext4 extent 写入支持 depth-2 index tree，
    // 并在重写/截断 tree 时释放旧 metadata block。
    // openat04 暴露 linkat(/proc/self/fd/N, ..., AT_SYMLINK_FOLLOW) 在解析
    // proc-fd magic link 前错误返回 EXDEV；现在 linkat 对解析后的 inode 做
    // cross-mount 检查，匹配 Linux。
    // &super::UNRUN_FS_TASKS,
    // 已在 riscv64 musl+glibc 验证：fs_di、ftest01-08、inode01-02、
    // stream01-05 通过。fs_di 注册为 `fs_di -d /fs_di_ltp`，因为
    // submit_plan 条目按空白拆分且没有 shell 变量展开，/tmp/tmpfs 也太小，
    // 放不下它的 30MiB 数据文件。
    // 预期 TCONF：fs_fill 报 tmpfs 可用内存不足，squashfs01 报当前镜像缺少
    // mksquashfs。fs_di 在深层随机目录树上仍打印非致命 coreutils chmod -R
    // EBADF/fts_read 警告；数据完整性检查通过，后续 chmod/fts 元数据批次应清理。
    // &super::UNRUN_FS_RACER_TASKS,
    // 已在 riscv64 musl+glibc 验证：fs_racer.sh 的 bounded
    // concat,rm、create,dir、rename,link,symlink、list 子组（各 `-t 1`/`-t 3`）
    // 通过；最终八条 UNRUN_FS_RACER_TASKS 组合回归每个 libc lane 均为
    // 8 RUN/8 PASS。日志归档：
    // .tmp/output-fs-racer-list-focused-20260629-193637.md
    // sha256=a11393aca34549a84db20c19d3c0246acf6412645837ba5dc198311784781100。
    // 本轮修复：dup3 替换目标 fd 时不再在 fd 表锁内析构旧 File，避免 pipe
    // close/wake 路径与 fd 表锁形成锁序阻塞。
    // cleanup 阶段可能打印非致命 "Directory not empty"/"no process found" 警告，
    // 但 LTP driver 给出 PASS 与 ALL TESTS DONE；本轮 subagent 已复核。
    // &super::UNRUN_FS_LINK_TASKS,
    // 已在 riscv64 musl+glibc 验证：linktest.sh 默认 1000 个 symlink 与
    // 1000 个 hardlink 均通过，两个 lane 均为 passed=2/failed=0。
    // 日志归档：.tmp/output-linktest-focused-pass-20260629-1920.md
    // sha256=3fc25c05ef9fc0a3519d1b4dd560e184fa160035e9ff2e79f06d74d4637ddf3f。
    //
    // mount / namespace / fanotify / proc-sysfs
    // &super::NS_MOUNT_CORE_TASKS,
    // &super::MOUNT_API_TASKS,
    // &super::NS_MOUNT_FOLLOWUP_TASKS,
    // &super::PIDNS_MODULE_TASKS,
    // &super::FANOTIFY_CORE_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK：mountns01-04、
    // setns01、setns02、unshare01、unshare02 通过。setns02 的
    // CLONE_NEWUTS 子项预期 TCONF，IPC namespace 子项通过；timens01、
    // pidns01 仍因当前镜像缺少对应 namespace/kernel config 预期 TCONF。
    // 2026-06-28 namespace anchor 回归还清理了 musl ELF interpreter
    // e_flags 误判导致的 255，以及 /proc/config.gz zcat/gzip 探测链。
    // MOUNT_API_TASKS 已在 riscv64 运行：fanotify21-23 因缺少 fanotify/
    // debugfs/mkfs.ext2 支持预期 TCONF；mount01-07、umount01-03、
    // umount2_01-02、mount_setattr01、fsconfig01-03、fsopen01 在 glibc
    // 通过。mount02 曾暴露普通 mount 到已有 mountpoint 错误成功；现在目标
    // 已是 mountpoint 时新 mount 返回 EBUSY，匹配 Linux。mount07 在默认 musl
    // 仍失败，因为 musl realpath(3) 处理 nosymfollow 路径不同；同一测试在 glibc
    // 通过，内核 open/readlink/statfs 检查通过。
    // NS_MOUNT_FOLLOWUP_TASKS 已在 riscv64 musl+glibc 运行且无 FAIL/TBROK：
    // fsopen02、fsmount01-02、fspick01-02、open_tree01-02、move_mount01-02、
    // pidns04 通过。预期 TCONF：userns01/06 需要 libcap；userns02-05/07-08
    // 和 pidns02-03 需要当前镜像不支持的 namespace/kernel config。
    // PIDNS_MODULE_TASKS 已在 riscv64 musl+glibc 运行且无 FAIL/TBROK：
    // pidns05-06/10/13/16-17/30-32、getcpu01 通过。预期 TCONF：
    // pidns12/20 需要不支持的 namespace/kernel config；module 测试缺少
    // test .ko 文件或架构支持；membarrier01 在这里不支持。
    // FANOTIFY_CORE_TASKS 已在 riscv64 运行且无 FAIL/TBROK；fanotify01-20
    // 均因当前内核未配置 fanotify 报预期 TCONF，其中 fanotify13 还会跳过
    // overlayfs-on-tmpfs 子项。
    // &super::FS_BIND_BASE_REGRESSION_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK/TCONF/TSKIP/TWARN：
    // fs_bind01-08.sh、fs_bind07-2.sh 与 fs_bind_regression.sh 通过。
    // 该批覆盖 base bind shared-subtree 传播、private/slave/unbindable
    // parent/child 组合，以及 unshared mountpoint 上 bind/rbind/MS_MOVE
    // regression。fs_bind_regression.sh 三个子项均为 TPASS。
    // &super::FS_BIND_RBIND_PROPAGATION_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK/TCONF：
    // fs_bind_rbind01-08.sh 与 fs_bind_rbind07-2.sh 通过。该批覆盖
    // shared/slave/unbindable 递归 bind 传播、堆叠 bind mount 逐层卸载、
    // slave 不向 master 反向传播卸载，以及 /proc/mounts 供 umount 工具
    // 消费时只暴露可见顶层挂载。
    // &super::FS_BIND_RBIND_CHILD_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK/TCONF/TSKIP/TWARN：
    // fs_bind_rbind09-16.sh 通过。该批覆盖 slave child 递归 bind 到
    // shared/private/slave/unbindable parent，以及 unbindable child 递归
    // bind 到 shared/private/slave/unbindable parent 时按 Linux 语义拒绝
    // 克隆 unbindable subtree。
    // &super::FS_BIND_RBIND_SHARED_SUBTREE_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK/TCONF/TSKIP/TWARN：
    // fs_bind_rbind17-24.sh 通过；组合回归 fs_bind_rbind01-24.sh 也通过。
    // 该批覆盖 shared subtree 携带 shared/private child 递归 bind 到
    // shared/private/slave/unbindable subtree 时的子挂载传播、逐层卸载，
    // 以及 shared destination 下 private child clone 的新 peer group 语义。
    // &super::FS_BIND_RBIND_SPECIAL_CHILD_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK/TCONF/TSKIP/TWARN：
    // fs_bind_rbind25-32.sh 通过；组合回归 fs_bind_rbind01-32.sh 也通过。
    // 该批覆盖 shared subtree 携带 slave/unbindable child 递归 bind 到
    // shared/private/slave/unbindable subtree 时的传播边界，尤其是
    // rbind 父树时按 Linux 语义跳过 unbindable child subtree。
    // &super::FS_BIND_RBIND_TOPOLOGY_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK/TCONF/TSKIP/TWARN：
    // fs_bind_rbind33-39.sh 通过；组合回归 fs_bind_rbind01-39.sh 也通过。
    // 该批覆盖 same-tree root-to-child rbind、shared child clone、slave/
    // unbindable propagation 边界，以及 private parent 下 shared child 的
    // 逐层卸载。修复点：same-tree rbind 克隆 shared peer child 时区分
    // 原始 mount event 与本次 clone event，保留 Linux 式覆盖层卸载顺序；
    // covered-peer unmount 清理仅在 clone source_display 被改写为目标路径时
    // 使用真实 source 兜底匹配，避免误删仍需显式卸载的同源层。
    // &super::FS_BIND_CLONENS_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK/TCONF/TSKIP/TWARN：
    // fs_bind_cloneNS01-07.sh 通过；组合回归 fs_bind_rbind01-39.sh、
    // fs_bind_rbind07-2.sh 与 fs_bind_cloneNS01-07.sh 也通过。该批覆盖
    // clone 后父/子 mount namespace 之间的 shared/slave/private/
    // unbindable propagation、跨 namespace mount/umount fanout，以及
    // shared/slave 链上的卸载传播方向。修复点：shared peer unmount 只在
    // 起点 peer group 及其下游 slave/shared-slave peer group 内清理；没有
    // shared peer group 的 mount namespace clone 记录按当前 namespace 栈项卸载，
    // 避免反向删除 upstream master 层。
    // &super::FS_BIND_MAIN_FOLLOWUP_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK/TCONF：
    // fs_bind07-2.sh、fs_bind09-24.sh 通过。该批覆盖 shared/slave
    // p-node 传播、shared+slave master 继承、bind+propagation 组合标志、
    // same-tree bind/rbind 子树克隆、MS_MOVE 子树重挂载，以及被覆盖 peer
    // 挂载层的逐层卸载清理。
    // &super::FS_BIND_MOVE_CORE_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK/TCONF/TSKIP：
    // fs_bind_move01-12.sh 通过。该批覆盖 shared/private/slave/
    // unbindable subtree 移动到 shared/private/slave/unbindable parent 时的
    // 传播语义。修复点：shared subtree move 保留原 peer group identity，
    // 同时在目标 parent peer 下生成 move 副本；private source root 的普通
    // bind 不再误克隆 moved child mount，避免把非递归 bind 当成 rbind。
    // &super::FS_BIND_MOVE_NESTED_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK/TCONF/TSKIP：
    // fs_bind_move13-22.sh 通过；组合回归 fs_bind_move01-22.sh 也通过。
    // 该批覆盖 unbindable subtree 的 move 拒绝、shared parent 下移动子树
    // 的拒绝、移动 private parent 及其 shared/slave/unbindable child 到
    // 既有 private mountpoint 时的 stack 语义，以及 shared tree 在自身
    // bind tree 内的嵌套 move。修复点：MS_MOVE 允许在目标 mountpoint 上
    // 形成新的顶层 stack，保留子挂载传播身份；同时按 Linux 语义拒绝
    // shared parent 下的 subtree move 和需要向 shared target fanout 的
    // unbindable subtree move。
    // &super::UNAME_SYSFS_ASLR_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK：newuname01、
    // utsname01-04、sysconf01、getpagesize01、syscall01 通过。预期 TCONF：
    // cve-2017-2618 需要 /proc/self/attr/fscreate；cve-2017-2671 需要
    // IPPROTO_ICMP sockets；cve-2022-4378/aslr01 需要不支持的 kernel config；
    // 旧 sysfs/_sysctl syscalls 在 riscv64 不可用；部分 libc sysconf
    // resource 不支持。
    // &super::IO_PERF_SYSINFO_PATHCONF_TASKS,
    // 已在 riscv64 验证：ioprio_get01、ioprio_set01-03、sysinfo01-02、
    // personality01-02、confstr01、pathconf01-02、fpathconf01 在 glibc
    // 通过；arch/misc 配置型用例在不支持处报告预期 TCONF。pathconf02 只在
    // 默认 musl 失败，因为 musl pathconf(_PC_LINK_MAX) 对若干错误路径返回
    // 静态限制，而不是先验证路径并返回 ENOTDIR/ENOENT/EACCES/ELOOP。
    // 预期 TCONF：ioperm/iopl 是 x86-only；perf_event 不支持；sysinfo03
    // 需要不支持的 kernel config；ioprio_set01 跳过可选 priority-decrease 探针。
    // &super::KCMP_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK：kcmp01 报预期 TCONF，
    // 因为该架构/镜像不支持 __NR_kcmp。
    // &super::UTS_NAME_TASKS,
    // &super::UTS_QUERY_TASKS,
    // &super::GETRANDOM_TASKS,
    // 已在 riscv64 musl+glibc 验证且无 FAIL/TBROK：sethostname01-03、
    // setdomainname01-03、gethostname01、getdomainname01、getrandom01-05
    // 通过，覆盖 setter 成功路径、EINVAL/EFAULT/EPERM 探针，以及 getrandom
    // 无效 buffer/无效 flag 检查。
    //
    // IPC / POSIX MQ / SysV IPC
    // &super::SYSV_SHM_CORE_TASKS,
    // 已在 riscv64 musl+glibc 验证：shmat02-04、shmctl01-08、
    // shmdt01-02、shmget03-06、shmt02 通过。预期 LTP TCONF/skip：
    // shmctl02 的 libc EFAULT variant 跳过；shmctl04 SHM_STAT_ANY；
    // shmctl05 remap_file_pages；shmctl06 shmid64 time_high；
    // shmget05-06 CONFIG_CHECKPOINT_RESTORE。shmat1 留给单独的调度器/
    // runtime 排查，因为这个旧 pthread 压测用例会在未同步 done_shmat
    // 交接尾部附近卡住。
    // &super::SYSV_SHM_FOLLOWUP_TASKS,
    // 已在 riscv64 验证：glibc 通过 shmget02 与 shmt03-10；musl 通过
    // shmget02、shmt03-08、shmt10。musl shmt09 在第一次 sbrk() 处失败，
    // 未进入内核 brk syscall，因此归类为 libc/runtime wrapper 限制；可选
    // libltp_sbrk_fix.so 保持可用但默认禁用。
    // &super::SYSV_IPC_CORE_TASKS,
    // 已在 riscv64 musl+glibc 验证：msgctl01-02、msgget01-02、msgrcv01、
    // msgsnd01、semctl01-02、semop01-02、semget01、shmat01 通过。
    // semop02 在 plain semop variant 下对 semtimedop-only 用例有预期 TCONF。
    // &super::SYSV_IPC_EXT_TASKS,
    // 已在 riscv64 musl+glibc 验证：msgctl03-06、msgget03-04、
    // msgrcv02-03、msgsnd02/05、semctl03-08、semop03-05、semget05 通过。
    // 预期 TCONF/skip：msgctl04 与 semctl03 的 libc EFAULT variant；
    // msgctl05/semctl08 time_high 字段；msgget04/msgrcv03 的
    // CONFIG_CHECKPOINT_RESTORE。
    // &super::SYSV_MSG_STRESS_TASKS,
    // 尚未标记为已验证：msgstress01 功能上报告 TPASS，且 riscv64 musl+glibc
    // 都能收到所有消息，但两个 variant 在当前 harness 下都会输出 TWARN
    // "Out of runtime during forking" 并返回 4。该项按 stress/runtime 规模问题
    // 后续跟进，不视为消息队列正确性失败。
    // &super::IPC_NAMESPACE_TASKS,
    // 已在 riscv64 musl+glibc 验证：msg_comm、sem_comm、shm_comm、
    // shmem_2nstest、shmnstest、mesgq_nstest、sem_nstest、semtest_2ns
    // 通过。mqns_01-04 在当前镜像中为预期 CONFIG_USER_NS TCONF。
    // &super::POSIX_MQ_SYSV_MSG_SEM_TASKS,
    // 已在 riscv64 musl+glibc 验证：POSIX MQ 用例、msgctl12、msgrcv05-08、
    // msgsnd06、semctl09、semget02 通过。预期 TCONF/skip：16-bit
    // setuid/setreuid compat 用例在该平台不支持；msgget05 需要
    // CONFIG_CHECKPOINT_RESTORE。
    // &super::EXEC_FAMILY_CORE_TASKS,
    // vfork 在 musl+glibc 报预期 TCONF，因为 freezer helper 需要 ptrace
    // SETOPTIONS/TRACEVFORK 能力；vfork01/02 与 exec/execve/execveat 用例通过。
    // &super::CLONE_EXEC_CHROOT_GROUPS_TASKS,
    // 已在 glibc 验证；musl clone08 是预期 libc/runtime 限制，已记录在
    // submit_plan.rs。clone09 是预期 CONFIG_NET_NS TCONF；execveat03 在当前
    // 镜像中是预期 overlayfs TCONF。
    // &super::THREADING_PTRACE_TASKS,
    // set_thread_area01 与 ptrace04/07-10 在 riscv64 上为预期 arch/x86-only
    // TCONF；其余 threading/ptrace 用例在 musl+glibc 通过。
    // &super::PIDFD_PRCTL_TASKS,
    // 接受 /proc/<pid> 目录 fd 作为可发信号 pidfd 后，pidfd_send_signal01-02
    // 通过。其余非 PASS 用例为预期 arch/config TCONF：pidfd_getfd*、
    // pidfd_send_signal03，以及不支持的 prctl 选项。
    // &super::PROCINFO_TASKS,
    // 进程身份/信息 syscall 在 musl+glibc 通过：getpid/getppid、
    // uid/gid/euid/egid、getpgid/getsid、uname、gettimeofday EFAULT 用例。
    // &super::PGRP_SESSION_TASKS,
    // setpgid01-03 与 setsid01 在 musl+glibc 通过，覆盖进程组创建、
    // 无效 pid/pgid 错误、session 边界和 setsid EPERM。
    // &super::SETPGRP_TASKS,
    // legacy setpgrp wrapper 用例在 musl+glibc 通过，包含 setpgid(0,0)
    // 行为和进程组校验。
    // &super::GETPRIORITY_TASKS,
    // getpriority01-02 在 musl+glibc 通过，覆盖 PRIO_PROCESS/PGRP/USER 查询
    // 以及无效 which/pid 的 ESRCH/EINVAL 处理。
    // &super::SETPRIORITY_TASKS,
    // setpriority01-02 在 musl+glibc 上通过 PRIO_PROCESS/PGRP 和错误路径；
    // PRIO_USER 子项因缺少 add-user setup 为预期 TCONF。
    // &super::PROC_TID_TASKS,
    // getpgrp01 与 gettid01-02 在 musl+glibc 通过，覆盖不同于进程 leader 的
    // 唯一线程 TID 分配。
    // &super::SCHED_NICE_CORE_TASKS,
    // nice01-05 以及核心 sched_* priority/policy/affinity/yield 用例在 glibc
    // 通过。内核已实现 SCHED_RESET_ON_FORK；可选 /extra/libltp_sched_fix.so
    // adapter 保留给 bundled musl sched_* libc wrapper，否则这些 wrapper 会在
    // 未发 syscall 前返回 ENOSYS。该 adapter 在 submit_script.rs 中默认未启用。
    // &super::SCHED_TC_TASKS,
    // sched_tc0-6 在默认 harness 的 musl+glibc 下通过。
    // UNRUN_SCHED_TASKS 部分状态：
    // 加入受管理的 scheduler /proc/sys/kernel sysctl 文件后，proc_sched_rt01
    // 在 musl+glibc 通过，覆盖 sched_rt_period_us、sched_rt_runtime_us、
    // sched_rr_timeslice_ms。autogroup01 因不支持 autogroup 为预期 TCONF。
    // starvation 本批不计为通过；fair wakeup / syscall-return resched 修改后
    // 需要专项复验。
    // UNRUN_MATH_TASKS 探测状态：
    // atof01、float_bessel、float_exp_log、float_iperb、float_power 在
    // riscv64 musl 通过。完整组尚未验证：这些旧 float 用例是多线程 500-loop
    // CPU stress，且本次探测在单核 QEMU 中运行到 float_trigo 时停止。
    //
    // 高级文件 I/O / pipe / splice / AIO / io_uring
    // PIPE_CORE_TASKS 在 musl+glibc 通过，覆盖 pipe01-13，包括 nonblocking、
    // close wakeup 和 multi-reader 行为。
    // PIPE_SENDFILE_SPLICE_TASKS 在 musl+glibc 通过；splice05 因不支持
    // AF_UNIX socket splice 报预期 TCONF。
    // TEE_VMSPLICE_FADVISE_TASKS 在 musl+glibc 通过。预期 TCONF：
    // sendfile09/_64 需要 5G 可用空间；splice06 需要
    // /proc/sys/kernel/domainname；splice07 跳过 fanotify/inotify/io_uring/
    // memfd_secret 等不支持的 fd class；splice08/09 需要 kernel >= 6.7。
    // ext4 extent writeback 支持 depth-2 extent tree 后，
    // FALLOCATE_FSYNC_SYNC_TASKS 在 musl+glibc 通过，fsync02 的随机稀疏写
    // 能超过旧单索引 extent 容量。预期 TCONF：fallocate04 打洞、
    // fallocate05 不支持的 fallocate mode、fallocate06 tmpfs 内存限制，
    // 以及 readdir21 在 riscv64 缺少 __NR_readdir。
    // AIO_DIO_CORE_TASKS 在 musl+glibc 无真实失败。当前镜像中 AIO/libaio、
    // CONFIG_AIO、io cgroup、readahead syscall、/proc/self/io 用例为预期
    // TCONF；包含 dma_thread_diotest 的 DIO stress 用例通过。重 DIO 负载下
    // 仍会打印 virtqueue-stuck warning，但未转化为 LTP 失败。
    // 修复 linkat proc-fd/O_TMPFILE openat04 锁顺序后，
    // IOCTL_IOURING_OPEN_TASKS 无真实失败。当前镜像中 ioctl loop/sg/btrfs
    // 以及 openat2/io_uring 用例为预期 TCONF 或 ENOSYS。
    //
    // Commands / shell-wrapper tests
    // UNRUN_COMMANDS_TASKS has been probed on riscv64 musl+glibc, but the full
    // batch is not yet verified. Focused df01.sh now passes in both lanes with
    // no warnings after adding /proc/*/mountinfo, fixing umount busy checks to
    // use raw inode paths for open-fd ownership, and returning ENXIO for
    // LOOP_CLR_FD on the unattached /dev/root pseudo block device. Focused
    // du01.sh also passes after making symlink st_blocks follow Linux fast
    // symlink behavior. Focused cp_tests.sh now passes after making proc-fd
    // chmod operate on the fd target for O_PATH-compatible metadata updates and
    // preserving leading ".." in relative inode path walks. Focused ldd01.sh
    // now passes with /extra/bin/ldd dispatching to the real glibc loader first
    // and falling back to the real musl ldd when the glibc trace rejects a musl
    // ELF. Focused ln_tests.sh, mkdir_tests.sh, mv_tests.sh, wc01.sh, and
    // which01.sh also pass in both lanes. Focused ar01.sh, file01.sh,
    // gzip_tests.sh, and tar_tests.sh pass after adding /extra/bin/gunzip as
    // a real gzip -d command alias. Focused lsmod01.sh/sysctl01.sh/sysctl02.sh
    // have no failures: lsmod01 is TCONF without ltp_lsmod01.ko, sysctl01 is
    // TCONF without sched_time_avg(_ms), and sysctl02 keeps fs.file-max stable
    // for overflow writes before skipping the KALLSYMS/KASAN-gated subcase.
    // Focused gdb01.sh passes in both lanes, although gdb still reports
    // missing /proc/*/mem and reduced ptrace feature probing; ld01.sh and
    // nm01.sh are TCONF because gcc/nm are not in the image. Focused
    // cpio_tests.sh, unzip01.sh, mkfs01.sh, mkswap01.sh, keyctl01.sh, and
    // logrotate_tests.sh have no failures, but all skip as expected TCONF for
    // missing cpio, unzip, mkfs.tmpfs, uuidgen, keyctl, and crontab.
    // Focused sendfile01.sh now passes on riscv64 musl+glibc after making TCP
    // accept sleep on the listener poll queue and making close cooperatively
    // drain queued TCP data before removing the smoltcp handle. Focused
    // unshare01.sh passes on riscv64 musl+glibc after adding a real unshare
    // command, user namespace id-map views, /proc/sys/user/max_mnt_namespaces,
    // and Linux-like shared root mount propagation across CLONE_NEWNS; the 8
    // subtests are all TPASS, including --mount --propagation shared. Follow-up
    // NS_MOUNT_CORE_TASKS rerun also passes mountns01-04, setns01-02, and
    // unshare01-02 on riscv64 musl+glibc after relaxing ELF PT_INTERP e_flags
    // matching and routing gzip/gunzip/zcat through static busybox. Expected
    // TCONF remains for the setns02 CLONE_NEWUTS subcase and timens01/pidns01
    // config-gated cases.
    // Expected TCONF/tool gaps still include cpio, insmod/lsmod modules,
    // keyctl, gcc, crontab, mkfs.tmpfs, uuidgen, nm, unzip, and several sysctl
    // config gates.
    //
    // 网络 / socket / net command
    // NET_SOCKET_CONN_TASKS pass on riscv64 musl+glibc after rechecking with
    // CLONE_NEWUSER/CLONE_NEWNET enabled. bind06 now exercises the AF_PACKET
    // bind/SIOCSIFFLAGS race and passes in both lanes. Expected TCONF/skips:
    // socketcall* on riscv64, unsupported SCTP/UDP-Lite/IPv6, and optional fd
    // classes in accept/fanotify/io_uring/memfd_secret subcases.
    // Review follow-up recheck: socket(AF_INET, SOCK_RAW, 0) now returns
    // EPROTONOSUPPORT instead of creating a protocol-less raw socket, and
    // MCAST_JOIN_GROUP/MCAST_LEAVE_GROUP use the Linux multicast-group path
    // for TCP sockets. This fixes socket01/accept02 without fake success.
    // NET_SEND_RECV_TASKS pass on riscv64 musl+glibc after keeping
    // libc-wrapper sensitive recvmmsg01/sendmsg01 in
    // NET_SEND_RECV_GLIBC_ONLY_TASKS. Those two cases intentionally pass bad
    // user pointers; bundled musl dereferences them before the syscall, while
    // glibc reaches the kernel and validates Linux EFAULT behavior. Fixed
    // during this recheck: raw IPv4 sockets accept IP_HDRINCL/SO_PRIORITY
    // control options for sendmsg03, and AF_PACKET accepts PACKET_VERSION /
    // PACKET_RESERVE / PACKET_VNET_HDR / PACKET_RX_RING control paths with
    // Linux-style EINVAL for oversized reserve and unsupported vnet+ring,
    // letting sendto03 complete without TBROK. Expected TCONF: IPv6, RDS and
    // SCTP dependent subcases in this image.
    // NET_SOCKOPT_POLL_TASKS pass on musl+glibc. Fixed during this batch:
    // AF_PACKET PACKET_VERSION/PACKET_RX_RING/PACKET_RESERVE/clear-ring
    // control paths now follow Linux enough for setsockopt02/06/07/09,
    // including rejecting oversized tp_sizeof_priv, bounding reserve updates
    // after a ring exists, and getsockopt(PACKET_RESERVE). SO_NO_CHECK is
    // accepted for UDP sockets. Expected TCONF/skips: unsupported IPv6,
    // 32-bit compat-only setsockopt03 variant, optional sockopt kernel-config
    // subcases, and old select/__newselect/pselect6 time64 syscall variants
    // absent on riscv64.
    // &super::UNRUN_NET_IPV6_LIB_TASKS,
    // &super::UNRUN_NET_IPV6_LIB_GLIBC_ONLY_TASKS,
    // verified on riscv64 musl+glibc: getaddrinfo_01, in6_01, in6_02,
    // asapi_02, and asapi_03 pass; glibc-only asapi_01 also passes.
    // Fixed during this batch: real rootfs /etc is no longer shadowed by
    // pseudo /etc, the image now carries hosts/protocols/services NSS data,
    // and /proc/<pid>/status exposes VmData for LTP interface helpers.
    // Expected TCONF/skips: AF_INET6 raw/socket protocol paths are not
    // implemented in this image; bundled musl also lacks the "hopopt"
    // getprotobyname() table entry, so asapi_01 stays in the glibc lane.
    // UNRUN_VSOCK_TASKS pass on riscv64 musl+glibc. Fixed during this batch:
    // AF_VSOCK now exposes a Linux-compatible control plane for vsock01
    // (socket creation, SO_VM_SOCKETS_* buffer/timeout sockopts, sockaddr_vm
    // validation, and ECONNREFUSED for loopback closed ports instead of
    // ENODEV/success). /proc/config.gz now advertises CONFIG_VSOCKETS(_LOOPBACK)
    // only after that control plane exists. The image also carries the real
    // gzip binary plus the gzip-package zcat frontend so LTP kconfig parsing
    // actually decompresses /proc/config.gz. VSOCK data transport/listen/accept
    // remains intentionally unsupported and returns errors rather than fake
    // success.
    // UNRUN_NET_TC_ROUTE_TASKS pass on riscv64 musl+glibc after replacing the
    // incomplete image `ip` with real iproute2-minimal and rebuilding
    // route-change-netlink as a real static riscv64 helper with libmnl compiled
    // in. submit_plan runs the upstream route-change-netlink-{dst,gw,if}.sh
    // scripts, not the helper directly; all three create/move ltp_ns_veth*,
    // configure addresses, and complete 10000 route add/delete iterations.
    // Cleanup still prints non-fatal `ip addr flush ... Invalid argument`
    // warnings. Expected TCONF still includes nft/tc/icmp_rate_limit kconfig
    // gates and route4/route6-rmmod missing locale command. This batch does
    // not yet exercise nft or traffic-control runtime semantics.
    // UNRUN_NET_FEATURES_TASKS currently contains fanout01 only. It passes on
    // riscv64 musl+glibc after adding minimal CLONE_NEWUSER procfs control
    // files, CLONE_NEWNET unshare support, and AF_PACKET PACKET_FANOUT state
    // under the packet socket lock. This covers the fanout bind race control
    // path; real packet fanout distribution is not implemented yet.
    // UNRUN_CONTAINERS_TASKS currently contains netns_netlink only. It passes
    // on riscv64 musl+glibc after adding minimal /dev/net/tun TUNSETIFF /
    // TUNSETPERSIST control-plane support and RTMGRP_LINK multicast delivery
    // for TAP create/delete events. This does not yet implement TUN/TAP packet
    // data I/O.
    // UNRUN_NET_CMDS_BASIC_TASKS pass on riscv64 musl+glibc with real command
    // binaries, not output-normalizing wrappers: netstat01, ip_tests, ping01,
    // ping02, if-updown, if-mtu-change, if-addr-adddel, and if-route-adddel
    // all passed in both lanes. The image uses real riscv64
    // Alpine/iputils/net-tools/tcpdump plus GNU/procps/util-linux binaries for
    // this command surface. Kernel-side support used here includes Linux-like
    // /proc/<pid>/ns/net magic symlink stat, namespace-filtered rtnetlink
    // dumps, route/address/dev_mcast filters, and mutation ACK handling that
    // follows NLM_F_ACK. ping still prints non-fatal IP_RETOPTS warnings
    // because that legacy IP option is not implemented; LTP records no
    // FAIL/TBROK/TFAIL.
    // UNRUN_NET_STRESS_INTERFACE_V4_TASKS pass on riscv64 musl+glibc:
    // if-updown, if-addr-adddel, if-route-adddel, and if-mtu-change all pass
    // with both explicit iproute2 and net-tools command variants. Fixed during
    // this batch: IPv4 address labels/ifconfig aliases now follow Linux
    // ifa_label behavior for SIOCSIFADDR/SIOCDIFADDR/SIOCGIFCONF and alias
    // down deletion, and IPv4 SIOCADDRT/SIOCDELRT rtentry handling now covers
    // route(8)'s host/net add-delete paths. The route ioctl reads rt_dev
    // byte-by-byte so Linux-compatible user pointers near a page boundary do
    // not spuriously fail with EFAULT. MTU stress still prints non-fatal
    // IP_RETOPTS warnings, but all 400 ping probes per run pass.
    // UNRUN_NET_STRESS_ROUTE_TASKS pass on riscv64 musl+glibc with no
    // FAIL/TBROK/TFAIL. IPv4 route-change dst/gw/if, netlink dst/gw/if, and
    // route-redirect all pass in both lanes; IPv6 variants cleanly TCONF with
    // "IPv6 disabled" in this image. Runs still print non-fatal IP_RETOPTS
    // warnings from ping and killall usage text during route-redirect cleanup.
    // UNRUN_NET_IPSEC_TCP_SMOKE_TASKS was checked on riscv64 musl+glibc with no
    // FAIL/TBROK/TFAIL. The plain TCP netstress baseline passes all four size
    // points in both lanes; AH transport, ESP tunnel, and ESP/VTI tunnel cases
    // cleanly TCONF because the image/kernel does not provide xfrm_user.
    // This is a representative IPsec gate, not a full IPsec algorithm-matrix
    // completion record.
    // UNRUN_NET_IPSEC_UDP_ICMP_SMOKE_TASKS was checked on riscv64 musl+glibc
    // with no FAIL/TBROK/TFAIL. The plain UDP IPv4 netstress subcase passes in
    // both lanes, while the paired UDP-Lite subcase cleanly TCONF-skips because
    // AF_INET6/UDPLite is unavailable in this image. The plain ICMP flood smoke
    // passes all selected payload sizes in both lanes; AH/ESP/VTI variants
    // cleanly TCONF because the image/kernel does not provide xfrm_user. This
    // is a representative UDP/ICMP IPsec gate, not a full IPsec algorithm
    // matrix completion record.
    // UNRUN_NET_CMDS_TASKS was rechecked on riscv64 musl+glibc with no
    // FAIL/TBROK/TFAIL after removing command wrappers and restoring the
    // upstream tcpdump01.sh. The full double-libc batch is too slow for one
    // 900s run because route-change-* and if-mtu-change repeat many ping /
    // route mutations, so verification was split: the first full run covered
    // arping through ip_tests, focused reruns covered ipneigh01 arp/ip,
    // netns_comm and tcpdump01, musl completed the latter half, and glibc
    // completed route-change-dst/gw/if plus route-change-netlink-*,
    // route-redirect, tc01, tcp_fastopen_run, and tcpdump01. Expected TCONF
    // remains for optional kernel modules: ip_vti in icmp-uni-vti, ip_tables
    // in iptables01, sch_teql in tc01, and sch_netem in tcp_fastopen_run.
    // Fixed during this batch:
    // iproute2 named netns bind-mounts now pin the opened /proc/<pid>/ns/net
    // file on /var/run/netns/<name>, /var/run/netns exists in the image,
    // netdev/procfs/sysfs views are filtered by current net namespace, and
    // rtnetlink IFLA_NET_NS_PID/FD moves non-builtin links across namespaces.
    // route-redirect and multicast now use a real statically linked
    // ns-udpsender helper instead of an exit-0 stub. arping/tracepath/
    // traceroute/ping/tcpdump/arp/ifconfig/netstat/hexdump/modprobe,
    // route-change-netlink, cmp/find/mount/umount/xargs/zcat/telnet, and
    // pgrep/pkill/killall/sysctl are real riscv64 binaries, not
    // output-normalizing wrappers. They live under ext4-fs-packer/extra-riscv64
    // so loongarch64 images do not accidentally receive riscv64 ELFs; common
    // text config stays under ext4-fs-packer/extra. The arping01.sh overlay
    // only moves arping options before the destination address so the real
    // arping binary parses `-I/-f/-q` consistently; it does not change the
    // ARP success condition. The previous
    // route-change-netlink helper/libmnl packaging gap has been removed at
    // image level, and real iproute2-minimal now provides `/extra/bin/ip` so
    // route-change-netlink-* reaches the kernel route/netns paths. ip netns add
    // still prints a non-fatal flock warning because flock(2) is not
    // implemented.
    // tcpdump01.sh has been rechecked on riscv64 musl+glibc with the upstream
    // LTP script and real tcpdump/ping binaries. It passes after adding the
    // exercised AF_PACKET ring/mmap path, ICMP datagram ping socket support,
    // and a minimal default egress-device fallback for synthetic packet
    // observation.
    // UNRUN_NET_NFS_TASKS was checked on riscv64 musl+glibc with no FAIL/TBROK:
    // nfs01-09, nfslock01, and nfsstat01 all cleanly TCONF before reaching
    // kernel NFS semantics because the image lacks NFS userland tools such as
    // exportfs, nfsstat, and make.
    // UNRUN_NET_MULTICAST_TASKS pass on riscv64 musl+glibc with no
    // FAIL/TBROK/TFAIL after replacing ns-udpsender with the real LTP helper.
    // Static ns-mcast/ns-igmp helper binaries are overlaid into both LTP roots,
    // /proc/sys/net/{ipv4,core} exposes the multicast and socket-buffer
    // sysctls used by LTP, raw IPv4 IGMP sockets and multicast sockopts accept
    // the exercised control paths, UDP sockets grow buffers on demand instead
    // of preallocating large per-socket slabs, IP_PMTUDISC_WANT no longer
    // forces EMSGSIZE for large multicast UDP datagrams, and rtnetlink ignores
    // iproute2's zero-filled tail after nlmsg_len so `ip addr flush dev` does
    // not leave test interfaces down.
    // UNRUN_NET_STRESS_MULTICAST_TASKS pass on riscv64 musl+glibc with no
    // FAIL/TBROK/TFAIL. The IPv4 stress multicast group operation, packet
    // flood, and query flood cases pass in both lanes; every IPv6 variant
    // cleanly TCONF-skips because IPv6 is disabled in this image. Fixed during
    // this batch: the installed LTP mcast-lib cleanup now skips pkill when
    // TCONF happens before multicast helper command templates are initialized,
    // avoiding an empty `pkill -f` pattern during IPv6-disabled setup.
    // UNRUN_NET_BROKEN_IP_TASKS pass on riscv64 musl+glibc with no FAIL/TBROK:
    // IPv4 checksum/dstaddr/fragment/ihl/plen/protocol/version cases all pass.
    // Expected TCONF: broken_ip-nexthdr is IPv6-only and IPv6 is disabled in
    // this image. Fixed during this batch: static ns-icmpv4/v6_sender helpers
    // are overlaid in /extra/bin ahead of the dynamic sdcard copies, and
    // AF_PACKET SOCK_DGRAM now accepts Linux-style sockaddr_ll bind/sendto
    // ABI paths used by those helpers.
    // UNRUN_NET_TCP_CC_TASKS was checked on riscv64 musl+glibc with no
    // FAIL/TBROK: bbr01, bbr02, and dctcp01 all cleanly TCONF because the
    // kernel image does not provide the sch_netem qdisc driver. Userland
    // coverage was prepared for later net batches by overlaying arping,
    // tracepath/tracepath6, traceroute/traceroute6, nft, wg, ss, route,
    // ethtool, and mii-tool plus nft runtime libraries through
    // ext4-fs-packer/extra-riscv64; older command-missing TCONF notes should be
    // rechecked when those batches are rerun.
    // UNRUN_NET_SCTP_DCCP_TASKS was checked on riscv64 musl+glibc with no
    // FAIL/TBROK: dccp01 and sctp01 cleanly TCONF because their netstress
    // server paths require AF_INET6/SCTP/DCCP support that is absent in this
    // IPv6-disabled image.
    // UNRUN_NET_SCTP_TASKS was checked on riscv64 musl+glibc with no
    // FAIL/TBROK: the 1-to-1, basic, assoc, sockopt, send/recv, tcp-style,
    // and time-to-live SCTP cases all cleanly TCONF because the SCTP driver
    // is unavailable in this image.
    // UNRUN_NET_TUNNEL_TASKS was checked on riscv64 musl+glibc with no
    // FAIL/TBROK: fou01, geneve01-02, gre01-02, and sit01 cleanly TCONF
    // because the image/kernel does not provide FOU/GENEVE/GRE/SIT tunnel
    // driver support.
    // UNRUN_NET_OVERLAY_TASKS was checked on riscv64 musl+glibc with no
    // FAIL/TBROK: macsec01-03, mpls01-04, vlan01-03, and vxlan01-04 all
    // cleanly TCONF because the kernel/image lacks the corresponding macsec,
    // MPLS, VLAN, and VXLAN device support.
    // UNRUN_NET_VIRT_TASKS was checked on riscv64 musl+glibc. ipvlan01,
    // macvlan01, macvtap01, and busy_poll01-03 pass in both lanes after veth
    // IP forwarding, IPv6 dual-stack socket compatibility, net.core busy-poll
    // sysctls, per-socket SO_BUSY_POLL,
    // netns address-list capacity fixes, and rtnetlink control-plane support
    // for ipvlan/macvtap add/delete modes. WireGuard groundwork has started:
    // rtnetlink can create/delete a link/none `type wireguard` netdev and
    // generic-netlink can discover the `wireguard` family plus store/query
    // WG_CMD_SET_DEVICE metadata, including WG_CMD_GET_DEVICE dump-all and
    // stale config cleanup on RTM_DELLINK. The WireGuard control plane is now
    // split out of rtnetlink dispatch, and all-zero private keys clear the
    // stored identity like Linux. Peer endpoint, protocol-version, and
    // allowed-ips parsing now follow Linux's netlink policy more closely,
    // including remove/replace allowed-ip updates. Endpoint storage is typed
    // after validation, and the module has a longest-prefix allowed-ips lookup
    // hook for the future data plane. X25519 public-key derivation is wired for
    // WG_CMD_GET_DEVICE, and setting a private key clears/ignores peers whose
    // public key matches the device public key like Linux. A no_std
    // wireguard_crypto helper module now mirrors Linux Noise_IKpsk2 primitives:
    // initial chaining-key/hash, HMAC-BLAKE2s HKDF, mix_hash/mix_key/mix_psk,
    // ChaCha20-Poly1305 seal/open helpers, data/handshake packet length
    // constants, data/handshake packet build/parse helpers, TAI64N timestamp
    // generation, Linux-style session keypair derivation, and replay-window
    // counter validation. It also has internal helpers for the Noise mainline:
    // create initiation, consume initiation for a configured peer, create
    // response, and consume response, following Linux's e/es/s/ss/{t} and
    // e/ee/se/psk/{} ordering. Peer state also precomputes the static-static
    // X25519 shared value when device identity or peer keys change, matching
    // Linux's handshake preparation path. Handshake runtime now allocates
    // sender indexes, tracks pending initiations/established keypairs, validates
    // mac1, rejects stale initiation timestamps, and can build the corresponding
    // response packet. Established keypairs can now seal data packets for a
    // peer and open inbound data packets by receiver index, with replay-window
    // validation after AEAD succeeds. A minimal UDP tunnel data plane is wired
    // into the namespace loopback device: outbound IPv4 packets whose
    // destination matches WireGuard allowed-ips are encapsulated to the peer
    // endpoint, and inbound UDP packets for a WireGuard listen-port are
    // consumed as handshake/data packets, with decrypted inner IPv4 packets
    // requeued into the local namespace stack. Runtime recheck of
    // wireguard01.sh now passes on riscv64 musl+glibc: both lanes report
    // 24 passed, 0 failed/broken/skipped/warnings after fixing the first-packet
    // handshake race with a bounded pending-packet queue and reducing eager TCP
    // listener-buffer preallocation. wireguard02.sh also verifies the
    // WireGuard data path in both lanes; its IPsec comparison half cleanly
    // TCONF-skips at the real LTP driver gate because this image does not
    // provide xfrm_user/ip_vti. This is an honest remaining gap, not a fake
    // WireGuard pass: true full coverage needs real XFRM state/policy, VTI
    // netdev, and ESP/AH transform hooks.
    // busy_poll03 now reaches UDP netstress after wiring UDP receive into the
    // busy-poll path and making poll/receive waits use socket wait queues
    // instead of pure cooperative rescan loops. Poll waiter registration also
    // resets the per-handle readiness baseline when the previous waiter set
    // has drained, avoiding stale POLLIN state that can miss the next edge.
    // The SO_BUSY_POLL poll/select path now uses a lightweight net poll that
    // avoids address-list refresh and broad waiter notification in the short
    // busy-poll window, and ppoll directly returns the socket readiness found
    // by busy-poll instead of immediately running a second full scan.
    // The UDP-Lite half no longer TCONF-skips: IPPROTO_UDPLITE socket creation,
    // SOL_UDPLITE checksum coverage options, and SO_PROTOCOL reporting are now
    // wired. The smoltcp path also emits/accepts IP protocol 136, treats the
    // UDP-Lite length field as checksum coverage, validates coverage on RX,
    // calculates partial checksums on TX, and keeps UDP/UDP-Lite sockets in
    // separate receive and bind domains. A full virtual-net batch later showed
    // busy_poll03 was still slower than the normal wait path because the busy
    // loop did a net poll and then re-looked up the socket under a second netns
    // lock. The busy-poll path now mirrors Linux's receive-side shape more
    // closely by polling and checking the target socket readiness while holding
    // the same netns stack lock. poll/select busy-poll now uses the global
    // net.core.busy_poll window even when SO_BUSY_POLL was not set on the
    // socket, matching the busy_poll01 Linux sysctl path; recv-side busy-poll
    // remains tied to the per-socket window. Focused busy_poll01/03 now pass on
    // riscv64 musl+glibc with real TCP, UDP, and UDP-Lite traffic; latest run:
    // musl busy_poll01 +14%, musl busy_poll03 UDP +4% / UDP-Lite +7%, glibc
    // busy_poll01 +17%, glibc busy_poll03 UDP +3% / UDP-Lite +6%.
    // UNRUN_NFT_TASKS was checked on riscv64 musl+glibc with no FAIL/TBROK.
    // After adding real inetutils telnet to /extra/bin, nft01 reaches the
    // kernel capability probe and cleanly TCONF-skips because the nf_tables
    // driver is unavailable in this image.
    // UNRUN_TRACEROUTE_TASKS pass on riscv64 musl+glibc with real
    // tracepath/traceroute binaries. The LTP script overlays only keep command
    // syntax/output expectations compatible with current real tools:
    // tracepath options are placed before the destination, and modern
    // traceroute TCP SYN output may report 44-byte packets instead of the old
    // hardcoded 60-byte text. The tst_net.sh overlay makes the optional
    // `tst_require_drivers veth` helper call conditional so older shell-library
    // images do not fail before the real `ip link add ... type veth` capability
    // probe; veth absence still fails at the actual link creation. Kernel-side
    // support is real: veth peer traffic
    // resolves across namespaces, ICMP echo replies return to the sender, TCP
    // SYN probes receive a Linux-style closed-port RST, and UDP tracepath
    // probes to an unbound direct veth peer port now queue an
    // IP_RECVERR/MSG_ERRQUEUE ICMP Port Unreachable entry so tracepath reaches
    // the target and reports `hops 1`.
    // UNRUN_CAN_TASKS was checked on riscv64 musl+glibc with no FAIL/TBROK:
    // can_bcm01, can_filter, and can_rcv_own_msgs all cleanly TCONF before
    // reaching CAN socket semantics because the image/kernel does not provide
    // the vcan driver.
    // UNRUN_PTY_TASKS pass on riscv64 musl+glibc. Fixed during this batch:
    // PTY master/slave now use real bidirectional queues, devpts stat no
    // longer depends on whether the slave is unlocked for open, slave close
    // reports Linux-like master hangup/EIO, TIOCGWINSZ/TIOCSWINSZ share window
    // size state, FIONREAD reports queued bytes, TCFLSH/TCSBRK/TCXONC cover
    // the exercised control paths, and TIOCSETD rejects unsupported N_HDLC so
    // LTP records an honest TCONF rather than a fake success. Expected TCONF:
    // pty03 needs CONFIG_SERIO_SERPORT, pty04 needs TIOCVHANGUP, and pty06/07
    // need real system tty devices such as /dev/tty8.
    // UNRUN_WATCHQUEUE_TASKS pass on riscv64 musl+glibc as honest TCONF:
    // wqueue01-09 now stop at pipe2(O_NOTIFICATION_PIPE) with ENOPKG, matching
    // Linux when CONFIG_WATCH_QUEUE is unavailable. This fixes the previous
    // bad state where the flag was silently accepted as a normal pipe and the
    // tests later TBROKed on watch-queue ioctls. Keyctl/watch notification
    // delivery remains unimplemented; no fake notification pipe is exposed.
    // UNRUN_INPUT_TASKS pass on riscv64 musl+glibc as honest TCONF:
    // input01-06 all stop in input_helper because uinput is unavailable
    // (modprobe cannot find the module and /dev/uinput is absent). No fake
    // evdev/uinput device is exposed.
    // UNRUN_CRASHME_TASKS pass on riscv64 musl+glibc:
    // crash01 random-instruction fault handling and crash02 random syscall
    // argument fuzzing both complete with TPASS in both lanes; f00f is an
    // expected TCONF because that Pentium erratum check is i386-only.
    // UNRUN_UEVENT_TASKS pass on riscv64 musl+glibc as honest TCONF:
    // uevent01-03 stop at the LTP driver gate because loop/tun/uinput are not
    // available as real kernel drivers. The existing lightweight TUN control
    // plane is not advertised as a full tun driver and no fake kobject uevent
    // broadcast path is exposed.
    // UNRUN_IRQ_TASKS pass on riscv64 musl+glibc as honest TCONF:
    // irqbalance01 now stops at LTP's min-cpu gate because the running QEMU
    // image exposes only CPU0 online. Fixed during this batch: /proc/interrupts
    // and /proc/irq/*/smp_affinity exist, while sched_getaffinity and
    // /sys/devices/system/cpu/{online,present,possible} report the runtime
    // online hart mask instead of the MAX_HARTS build upper bound. No fake
    // irqbalance movement is reported.
    //
    // 内存管理
    // &super::MMAP_MPROTECT_CORE_TASKS,
    // 覆盖 brk01、sbrk01-02、mmap01-06、mmap09、mprotect01-02。
    // glibc 路径通过。musl 下 raw brk syscall 与 mmap/mprotect 通过；
    // 差异在 musl libc 的 brk/sbrk wrapper：brk01 的 libc 子项会 TCONF
    // （"brk() not implemented"），sbrk01 的 libc wrapper 路径会失败。
    // 这不归类为 MemorySet/brk syscall 语义失败。

    // &super::UNRUN_MALLOC_TASKS,
    // 覆盖 mallinfo01、mallocstress。mallocstress 在 musl/glibc 均通过；
    // mallinfo01 在 glibc 通过，musl 报预期 TCONF，因为 mallinfo() 是
    // 非 POSIX 接口，musl 不实现。相关修复保证 brk 不会跨过无关 mmap/
    // PROT_NONE 保留区，同时保留 mmapstress03 所需的旧 brk 区间内
    // MAP_FIXED 打洞行为。

    // &super::MLOCK_MADVISE_CORE_TASKS,
    // 覆盖 mlock01-03、mlockall01-02、munlock01-02、munlockall01、
    // madvise01-02、process_madvise01、mincore01。musl/glibc 均无真实
    // FAIL/TBROK。madvise01/02 中不支持的 advice 值按预期 TCONF；
    // process_madvise01 因当前镜像缺少 CONFIG_SWAP 报预期 TCONF。

    // &super::MPROTECT_MREMAP_MSYNC_TASKS,
    // 覆盖 brk02、sbrk03、mprotect03-05、munmap01-03、mremap01-06、
    // msync01-04、mincore02-03。musl/glibc 均通过。通过依赖于将 VMA
    // 元数据 split/trim/move/mprotect 操作收敛到 MemorySet API 后面，并
    // 修复 /proc/kpageflags 长度处理，使其接受 /proc/self/pagemap 给出的
    // 绝对 PFN 偏移。brk02 的 libc 子项与 sbrk03 在当前 LTP/libc 环境中
    // 仍为预期 TCONF。

    // &super::MM_MMAP_MADVISE_TASKS,
    // 覆盖 mmap-corruption01、mmap001、mmap08、mmap1-3、mmap10-20、
    // mmapstress01-10、mlock04-05、mlock201-203、mlockall03、
    // madvise03/05-11、mincore04、page01-02、memfd_create01-04。
    // glibc 路径已验证。musl 的 mmapstress02/03/05/06 在测试 setup 的
    // brk/sbrk 阶段因 ENOMEM 失败，归类为当前镜像的 libc/runtime 限制；
    // 同样用例在 glibc 通过。mmap3 保持 "-l 1 -n 4"，因为该 LTP case
    // 固定跑到 60s alarm，当前 QEMU 下更大轮次无法在 90s harness timeout
    // 前完成清理。普通 mmap/madvise、mmapstress09/10、mlock04/05、
    // mincore04、page01/02、memfd_create01/02 通过。mmap16、
    // mmapstress08、mlock201-203、madvise06-11 部分子项、memfd_create03/04
    // 因缺少 mkfs.ext4、架构限定、mlock2、memory-failure、memcg、coredump
    // 或 hugepage 支持而预期 TCONF。

    // &super::MM_OOM_TASKS,
    // 覆盖 oom01-05、overcommit_memory。oom01 与 overcommit_memory 在
    // musl/glibc 通过。修复点包括加入保守的 overcommit_memory=0 commit
    // limit，以及让 mlock 先预检 resident page population，避免耗尽所有
    // 物理页后才返回 ENOMEM。oom02-05 因 bundled LTP 缺少 libnuma 开发
    // 支持而预期 TCONF。

    // UNRUN_MM_TASKS 状态：
    // 覆盖 data_space、kallsyms、ksm01-07、max_map_count、mem02、
    // min_free_kbytes、mtest01、stack_space、swapping01、thp01-04、
    // vma01-04。data_space、kallsyms、mem02、mtest01、stack_space、thp01、
    // vma01、min_free_kbytes、max_map_count 在 musl/glibc 通过。
    // data_space/stack_space 覆盖旧式多子进程 VM data/stack 写入校验；
    // mem02 覆盖 calloc/malloc/realloc/valloc 到 64MB 以及 15000 次 valloc；
    // mtest01 在两种 libc 下分配 20% 空闲 RAM。ksm01/03/05/07 因缺少
    // CONFIG_KSM 预期 TCONF；ksm02/04/06 因 bundled LTP 缺少 libnuma 开发
    // 支持预期 TCONF。swapping01 因缺少 CONFIG_SWAP 预期 TCONF。
    // thp02-04 因当前内核配置不支持 transparent huge pages / huge pages
    // 预期 TCONF。vma02/vma04 因缺少 libnuma 开发支持预期 TCONF；vma03
    // 在 riscv64 上是 32-bit mmap2 overflow 回归测试，预期 TCONF。
    // min_free_kbytes 现在用 managed RAM 计算 CommitLimit，并使
    // /proc/sys/vm/min_free_kbytes 成为真实可写 sysctl；strict
    // overcommit_memory=2 在 mmap 时返回 ENOMEM，而不是 lazy-fault 后 OOM
    // SIGKILL。max_map_count 在 MAP_SHARED anonymous mmap 改为 lazy、
    // procfs 缓存大型 /proc/<pid>/maps 快照后通过。vma05.sh 因当前镜像
    // 缺少 gdb 在两种 libc 下预期 TCONF；真实运行还需要 coredump 与
    // /proc/sys/kernel/core_uses_pid 支持，vdso core 检查才有意义。

    // UNRUN_NUMA_TASKS / UNRUN_NUMA_SHELL_TASKS:
    // migrate_pages*、move_pages*、set_mempolicy01-04 需要 libnuma 开发
    // 支持，当前两种 libc 均预期 TCONF；set_mempolicy05 被 LTP 架构条件
    // 跳过。numa01.sh 因当前镜像缺少 bc 预期 TCONF。

    // UNRUN_HUGETLB_TASKS:
    // hugetlbfs / huge pages 在当前镜像中不支持，因此 hugefallocate*、
    // hugefork*、hugemmap*、hugeshm* 相关测试均预期 TCONF；hugeshmat04
    // 还要求至少 2048MB MemAvailable。
## 内存管理单项记录

- `MMAP_MPROTECT_CORE_TASKS`：按内存管理结果标记已通过。
  RISC-V glibc lane 已实测全部 `TPASS`，覆盖 `brk01`、`sbrk01`、
  `sbrk02`、`mmap01`、`mmap02`、`mmap03`、`mmap04`、`mmap05`、
  `mmap06`、`mmap09`、`mprotect01`、`mprotect02`。RISC-V musl lane
  的早期记录曾因内核把 PT_INTERP `e_flags` float ABI bits 与主程序做
  等值校验而返回 `255`；2026-06-28 已改为 Linux-like interpreter
  架构检查，不再要求这些 flag 相等。该旧 `255` 不归类为
  MemorySet/brk/mmap/mprotect 语义失败；musl lane 需要用新内核重跑后再
  记录最终 TPASS/TCONF 结果。

- `mmapstress09 -d -p 2 -t 1`：按内存管理结果标记已通过。
  RISC-V glibc lane 已实测 `TPASS`；RISC-V musl lane 的早期 `255`
  同样属于旧 PT_INTERP `e_flags` 等值校验问题。2026-06-28 该加载器
  语义已按 Linux 调整为 interpreter 架构检查；旧失败不归类为
  MemorySet/mmap 语义失败，musl lane 需要用新内核重跑后再记录最终结果。

## 内存管理 修改计划

目标：继续把当前 `MemorySet` 往 Linux `mm_struct` / `vm_area_struct`
模型推进。短期重点不是再多堆单点 case，而是减少双结构漂移风险，补齐
file-backed mmap、page fault、CLONE_VM 等核心语义。

### 短期保障线：先保证最基本功能稳定

当前计划先收敛到“基本可运行、核心 mmap 语义不回退”的版本，再继续推进完整
Linux-like `address_space`。本阶段不把 page reclaim、完整 swap/rmap、NUMA、
hugepage、精确全局 writeback 调度等长期 Linux mm 能力列为阻断项。

最基本功能定义：

- init/exec/fork/COW/brk 能正常支撑用户态启动和常规程序运行。
- 匿名 `mmap`、file-backed `mmap`、`munmap`、`mprotect`、`mremap`、`msync`
  的常见路径不破坏 `VmRegion`/`MapArea` 一致性。
- lazy fault、COW fault、SIGBUS tail、grow-down stack guard、PROT_NONE
  saved flags 等页故障路径必须由权威 VMA 元数据约束，不能重新退回
  `MapArea` 作为 policy 来源。
- OSInode-backed `MAP_SHARED` 至少保证基础跨 mm 可见性：已 fault 页的
  fd-write 镜像、跨 mm fault 复用 shared cache、truncate shrink/regrow
  不复用陈旧脏页。
- 非线程 `CLONE_VM` 至少共享同一个 `MmRef`，不会因为独立快照导致 mmap/
  munmap/mprotect/brk 元数据分裂。

本轮优先级调整：

1. 先修复会影响启动、exec/fork、brk、mmap/mprotect/mremap/msync、COW/lazy
   fault 的阻断性回归。
2. 再补齐会直接影响 LTP mmap/msync/mremap/fork 族的基础语义。
3. 最后才继续推进完整 Linux `address_space`：dirty write-protect fault、
   dirty-only writeback、page reclaim、全局 backing 生命周期和更复杂的
   truncate 并发语义。

最小验收线：

- `cargo fmt` 不产生额外格式差异。
- riscv64 与 loongarch64 的 `os` cargo check 通过。
- riscv64 focused memory smoke 在 release 和 debug build 下通过，debug
  VM invariant 不触发。
- 至少覆盖本地 smoke：file mmap lazy fault、shared file alias/cross-mm/
  kernel-write/fault-cache/truncate-cache、COW mprotect、CLONE_VM mmap/SysV shm、
  memfd/SysV shm mremap、mmap placement、growsdown guard、stack/private-file
  `MADV_DONTNEED`。
- 提交前恢复所有临时聚焦开关，尤其是 `submit_script.rs`/`submit_plan.rs`
  里的 focused/debug narrowing。

### 第一阶段：降低 `VmRegion` / `MapArea` 双结构风险

- 让 `VmRegion` 成为权威 VMA 描述，`MapArea` 只保存页表和物理页等 concrete state。
- syscall 层后续不再直接拼 `MapArea`，统一通过 `MemorySet` 的 VMA API 修改地址空间。
- 继续加强 debug invariant：覆盖区间、权限、映射类型、`file_valid_len`、
  SIGBUS tail、PROT_NONE 保存位、COW/SHARED 软件 PTE 位都要能交叉检查。
- 验证入口：debug runtime 的 `init_proc` 后 fork 卡住问题已修复；
  debug build 已跑通 `mmap01/mmap18/munmap01-03/mremap01-06/msync01-04/mprotect03-05`，
  后续再补 `mallocstress`、`mmapstress`。

### 第二阶段：补齐 file-backed mmap 语义

- 把当前 best-effort 的 write -> mmap 镜像，升级成真正的文件映射 backing 管理。
- 一个 file mapping identity 需要知道关联 VMA、已 materialized 页、有效文件字节范围和脏页状态。
- 文件通过 `write/pwrite/truncate/ftruncate` 增长后，要更新命中 VMA；新变为有效的页再次 fault 时应能加载文件内容，而不是永远停在旧 SIGBUS tail。
- `msync/munmap` 必须继续避免把 zero-fill EOF tail 当作真实文件内容写回。
- 已推进：`MmapBacking` 已开始记录 resident file pages 和 dirty hint；
  shared-file `munmap/msync` 的 resident 页扫描、EOF clamp、OSInode writeback、
  dirty 清理已收进 `MemorySet::writeback_shared_file_mmap_range()`。该路径还能覆盖
  `mprotect(PROT_NONE)` 后仍由 `MapArea` 保存 frame tracker 的 resident 页。当前 dirty
  hint 仍只作为状态记录和 msync 清理目标，不用来过滤写回；OSInode `pwrite64`
  以及 `sendfile`、`splice`、`copy_file_range` 的 kernel-buffer 写出已开始广播到
  所有进程 resident `MAP_SHARED` 页，覆盖跨 mm 已 fault 页的 fd-write coherence；
  普通 OSInode-backed `MAP_SHARED` fault 已有第一阶段全局 shared file page cache，
  可以让跨 mm fault 复用同一 file page frame；inode size 更新也会对该 cache
  做 EOF tail 清零和越界页移除，覆盖 shrink/regrow stale-cache 路径。但完整 Linux
  `address_space` 生命周期仍未完成，包括 page reclaim、精确写保护 dirty fault、
  脏页合并、writeback 过滤和更完整的 truncate 并发语义。
- 验证入口：`mmap01`、`msync01-04`、write/pwrite/truncate/ftruncate 相关 LTP，以及本地同页/跨页文件扩展 mmap 探针。

### 第三阶段：事务化 mmap / mremap 修改（部分推进）

- 在移动或覆盖用户可见映射前，尽量预检查地址、backing、frame 分配和 metadata 修改是否可行。
- 无法完全预检查的路径，需要 rollback object，统一回滚 PTE、`MapArea`、`VmRegion`、locked ranges 和 file-valid metadata。
- 先扩展到 `mremap(MAYMOVE)`、`MAP_FIXED` 替换、shared-frame 插入、brk 带洞增长和 file-backed grow。
- 已完成部分：`UserRangeRollback` 已覆盖 `mremap(MREMAP_FIXED)` 替换和
  `mremap` grow/relocate 失败路径；`brk` 带洞 grow/shrink 已收敛到
  `MemorySet::try_update_brk_with_holes()`，在单次 mm lock 下统一处理插入、
  回滚和最终 brk 更新；普通非 fixed file-backed `mmap` 的插入/populate 也已
  通过 `try_insert_user_vma_with()` 使用同一类 rollback hook；OSInode-backed
  file-backed `mremap` grow 已支持多段 grow-area、精确 file_valid_len 更新和
  EOF/SIGBUS tail rollback；shared memfd/POSIX shm `mremap` grow 已用同一
  rollback 路径插入 shared-frame 有效段和 SIGBUS tail；SysV shm `mremap` grow
  已按 segment 边界插入 shared-frame 有效段和 SIGBUS tail，并通过
  attach_id/accounted fragment 支持部分 attachment split；SysV shm 的
  `MAP_FIXED`、`mremap(MREMAP_FIXED)` 和 `shmat(SHM_REMAP)` 覆盖目标路径已先在
  cloned attach 表上 detach/split 元数据，`MemorySet` 替换提交成功后再提交
  attach 表并释放旧 segment 的 `nattch`，失败路径不提前收账；POSIX shm/memfd
  没有 SysV 式 attach 表，相关收账点是 `mmap_backings` 的文件引用。成功
  `munmap`/`MAP_FIXED`/`mremap(MREMAP_FIXED)` 删除 VMA 后，MemorySet 现在会
  剪掉不再被任何 VmRegion 引用的 backing id，debug invariant 也会同时检查
  VMA 的 backing_id 存在且不存在孤儿 backing；`UserRangeRollback` 的
  `UserRangeSnapshot` 也不再复制整张 `mmap_backings`，而是只保存捕获范围内
  VMA 切片实际引用的 backing entry，恢复时只把这些本地 entry 插回，避免
  失败回滚覆盖范围外 mmap backing 的生命周期状态；恢复后的
  `next_mmap_backing_id` 取快照旧值和当前 live backing 最大 id 的下一位中较大者，
  避免未来更通用事务在范围外创建 backing 后发生 id 复用。
- 验证：最终版本已跑 `cargo fmt`、riscv64/loongarch64 `os` cargo check，
  以及 focused riscv64 `MPROTECT_MREMAP_MSYNC_TASKS`；mprotect03-05、
  munmap01-03、mremap01-06、msync01-04、mincore02-03 在 musl/glibc 均通过，
  `find_ltp_error.py` 只报告既有 brk02/arch-unknown TCONF。`next_mmap_backing_id`
  加固前还跑过 release/debug memory smoke，file_mmap/clone_vm/memfd/SysV shm
  探针均通过且 debug VM invariant 未触发。
- 后续推进：`mmap_backings` 已从 `backing_id -> Arc<File>` 改成
  `backing_id -> MmapBacking { kind, file }`。普通 OSInode-backed mmap 的
  backing 记录 dev/ino，memfd/POSIX shm 记录 memfd_id；backing 分配从
  `VmRegion` 生成 identity，debug invariant 和 release 剪枝路径都会校验
  backing identity 与引用它的 VMA 匹配。`mmap_backing_file()` 继续作为外部
  文件句柄访问 API，避免把新结构扩散到 syscall 层。
- 验证：本轮 backing record 改造已跑 `cargo fmt`、riscv64/loongarch64 `os`
  cargo check、riscv64 release `os` cargo check；focused riscv64 debug
  memory smoke 中 file_mmap/clone_vm/memfd/SysV shm 探针均通过且新增 backing
  identity invariant 未触发；focused riscv64 `MPROTECT_MREMAP_MSYNC_TASKS`
  继续通过，`find_ltp_error.py` 只报告既有 brk02/arch-unknown TCONF。
- 第六阶段小步推进：`resolve_lazy_fault` 现在必须先命中
  `VmRegion`，访问权限、SIGBUS tail 和 PTE flags 都由 VMA 元数据生成；
  `MapArea` 只作为 resident/lazy concrete cache 来定位需要装页的 VPN，不再在
  缺页时充当无 VMA fallback 的权限来源。file-backed framed VMA 仍允许文件增长后
  新变为有效的 lazy concrete 子段按 VMA 的 file_offset/backing 读入，这是当前
  file-backed 表示法的兼容点。
- 验证：本轮 VMA-driven lazy fault 改造已跑 `cargo fmt`、riscv64/loongarch64
  `os` cargo check；focused riscv64 debug memory smoke 中
  file_mmap/clone_vm/memfd/SysV shm 探针均通过，且 debug VM invariant 未触发。
- 第六阶段继续推进：COW fault resolver 现在也先检查覆盖 `VmRegion`，只有
  当前 VMA 仍允许写、不是 shared mapping、且不在 SIGBUS tail 时才会把
  `PTEFlags::COW` 解析成私有可写页。这修掉了 fork 后对 COW 页
  `mprotect(PROT_READ)` 时旧 COW 软件位可能绕过 VMA 写权限的问题。debug VM
  invariant 也新增 resident/saved PTE flags 检查，覆盖 COW/SHARED 软件位、
  writable/executable PTE 与 VMA policy 的漂移。
- 验证：新增 `/user/cow_mprotect_smoke.bin` 覆盖 read-only VMA 下 COW 写 fault
  必须 SIGSEGV、父进程 writable VMA 仍可正常 COW 写入的路径；focused riscv64
  debug memory smoke 中 file_mmap/cow_mprotect/clone_vm/memfd/SysV shm 探针均通过。
- 第五阶段过渡推进：在保留 `vm_regions: Vec<VmRegion>` 作为当前权威结构的前提下，
  先利用“排序且不重叠”的 invariant 增加二分定位 helper。`resolve_lazy_fault`、
  `resolve_cow_fault`、SIGBUS tail 判断、mremap 的 VMA containment、共享 VMA
  overlap、brk 页级 overlap 等热点路径现在都先二分定位候选 VMA，再只扫描重叠窗口，
  不再每次从头扫完整 VMA 列表。`page_overlaps_mmap_region_started_before` 的边界也
  从 `region.start <= old_end` 收紧为 `region.start < old_end`，避免 mmap 正好贴着旧
  brk 末尾时被误认为旧 brk 内 hole。
- 验证：本轮 sorted-Vec lookup 改造已跑 `cargo fmt`、riscv64/loongarch64 `os`
  cargo check、riscv64 release `os` cargo check；focused riscv64 debug memory
  smoke 中 file_mmap/cow_mprotect/clone_vm/memfd/SysV shm 探针均通过，debug VM
  invariant 未触发。
- 第五阶段继续推进：mmap placement/free-area search 也收进 `MemorySet`。非 fixed
  `mmap` 的 hint fallback、`mremap(MAYMOVE)` relocation 搜索和 SysV
  `shmat(NULL)` 默认选址现在都通过 `MemorySet::find_free_mmap_range()`，
  由 mm 同时检查 concrete `MapArea`、权威 `VmRegion` 和 kernel-only 页表区域；
  syscall 层不再自己拼 `occupied_user_ranges_with_metadata()`/trim/source-exclude
  再调用独立 free-search helper。`mremap(MAYMOVE)` 的自动 relocation 选择
  与 source VMA 不重叠的目标空洞，避免滑动式重叠搬迁超出现有 move/grow
  事务模型。
- 验证：新增 `/user/mmap_placement_smoke.bin` 覆盖 occupied hint fallback、
  `shmat(NULL)` 跳过被占用的 `mmap_next` 候选页、以及 in-place grow 被阻塞时
  `mremap(MAYMOVE)` relocate 的路径；focused riscv64 debug memory smoke 6 个
  探针均通过。并已跑 `cargo fmt`、riscv64/loongarch64 `os` cargo check、
  riscv64 release `os` cargo check。
- 第五阶段继续推进：补上 Linux-like grow-down stack guard gap。`MAP_GROWSDOWN`
  VMA 下方现在保留 `USER_STACK_GUARD_GAP` 作为非 fixed mmap/shmat/mremap
  relocation 不可选区域；实际向下扩展时也会检查 guard gap 内是否已有
  `MapArea`/`VmRegion`，避免栈增长穿过下方已有映射。riscv64 与 loongarch64
  trap 路径都已接入同一套 `try_expand_growsdown()`。`MAP_FIXED`/
  `MAP_FIXED_NOREPLACE` 仍只按真实映射冲突处理，不把 guard gap 当成已映射页。
- 验证：新增 `/user/growsdown_guard_smoke.bin` 覆盖非 fixed hint 不能落入
  grow-down guard gap、无 blocker 时子进程栈下探可正常扩展，以及 guard gap 中部
  blocker 存在时对子进程栈下探 fault 必须被 SIGSEGV/SIGBUS 类信号终止；focused
  riscv64 debug memory smoke 7 个探针均通过。并已跑 `cargo fmt`、
  riscv64/loongarch64 `os` cargo check、riscv64 release `os` cargo check。
- 第五阶段继续推进：非 fixed mmap 默认布局改成 Linux-like top-down 搜索。hint
  仍优先按用户给定地址尝试；无可用 hint 时先从 `USER_VA_TOP` 向下找空洞，
  下界不低于 `max(DEFAULT_MMAP_BASE, brk + USER_HEAP_GAP)`，找不到再回落到
  brk gap 之上的 bottom-up 搜索。`mremap(MAYMOVE)` relocation 复用同一布局策略，
  但不把 source range 当成可占用目标，保证搬迁目标和旧映射不重叠。同步修复
  `user_range_fully_mapped()` 对 `[start, USER_VA_TOP)` exclusive end 的判断：
  该函数现在使用 `VirtAddr` 的原始低地址值，不再把 `1 << 38` 通过 Sv39
  sign-extension 转成高半 canonical 地址，从而允许顶页映射被正确识别为完整 VMA。
- 验证：`/user/mmap_placement_smoke.bin` 已覆盖连续非 fixed mmap 递减、occupied
  hint fallback、`shmat(NULL)` 跳过占用页，以及 `mremap(MAYMOVE)` 在 top-down
  布局下搬到更低的非重叠空洞；focused riscv64 debug memory smoke 7 个探针均通过，
  覆盖 file_mmap/cow_mprotect/clone_vm/memfd/SysV shm/mmap placement/growsdown。
- 第五阶段预备推进：先新增 `VmRegionSet` 封装；该步底层仍是 sorted Vec，但
  `MemorySet` 外层不再直接依赖 `vm_regions[idx]`、`len()+idx++`、`take()` 或
  `iter_mut()` 这类 Vec 细节。containing/overlap/snapshot/trim/mprotect/move/
  isolate/set_len/file-valid 更新和 growsdown start 变更都已收进 `VmRegionSet`
  的语义 API；后续把底层替换为 `BTreeMap<start, VmRegion>` 或 interval tree 时，
  主要改动面应集中在该封装内。
- 验证：本轮 `VmRegionSet` 语义 API 收口已跑 `cargo fmt`、riscv64 `os`
  debug/release cargo check、loongarch64 `os` cargo check（本机已安装 target 为
  `loongarch64-unknown-none`）；focused riscv64 debug memory smoke 7 个探针全部
  通过，debug VM invariant 未触发。
- 第五阶段继续推进：`VmRegionSet` 的权威存储已从 sorted Vec 切到
  `BTreeMap<start, VmRegion>`。VMA containing lookup 现在用 predecessor 查询，
  overlap/snapshot/共享映射扫描用 predecessor + start-key range 查询；grow-down
  扩展这种会改变 VMA start key 的路径改为 remove + expand + reinsert，避免在
  tree 中原地修改 key。snapshot/rollback 仍保留 `Vec<VmRegion>` 作为事务记录格式，
  但该 Vec 不再是地址空间 VMA 的权威结构。
- 验证：本轮 tree-backed VMA 迁移已跑 `cargo fmt`、riscv64 `os`
  debug/release cargo check、loongarch64 `os` cargo check；focused riscv64 debug
  memory smoke 7 个探针全部通过，debug VM invariant 未触发。
- 第五阶段继续推进：`trim/mprotect/move/isolate/set_len` 已改成 tree-local
  split/merge，不再把整棵 `VmRegionSet` 物化成 Vec 后全量 normalize。普通插入
  通过 `insert_merged()` 只检查相邻 predecessor/successor；`isolate` 使用
  `insert_unmerged()` 明确保留 split 边界，避免 grow-in-place 前的 VMA 边界被重新
  合并。`move_range_metadata_raw()` 也按每个重叠 VMA 的交集计算目标偏移，后续
  即使移动范围跨多个 VMA 也不会把所有切片放到同一个目标 start。
- 验证：本轮 tree-local VMA mutation 已跑 `cargo fmt`、riscv64 `os`
  debug/release cargo check、loongarch64 `os` cargo check；focused riscv64 debug
  memory smoke 7 个探针全部通过，debug VM invariant 未触发。
- 剩余：继续把更多跨步 mmap 修改改成单一 VMA transaction，并把
  当前 `MmapBacking` 从 identity/file holder 继续演进为 VMA/backing 生命周期
  统一对象；暂不把 refs、dirty tracking、materialized page 状态一次性迁入，
  这些仍需等 file-backed page-cache/backing 管理设计清楚后分阶段推进。
- 验证入口：`mremap01-06`、mmap fixed/noreplace、`brk02`、`mallocstress`、OOM/overcommit 聚焦组。

### 第四阶段：修正真正的 CLONE_VM 共享 mm（主体已完成）

- pthread 风格 `CLONE_THREAD | CLONE_VM` 继续共享同一个 PCB 和 `MemorySet`。
- 非线程型 `clone(CLONE_VM)` 已改为共享 `MmRef(Arc<Mutex<MemorySet>>)`，不再快照独立 `MemorySet`。
- trap-context slot 已改为 mm-local 分配，避免多个独立 PCB 的 tid 0 在同一共享 mm 中撞同一 TRAP_CONTEXT VA。
- process-private 与 mm-shared 资源已初步拆开：信号、fd、父子关系跟 clone flags 走；VMA、brk、mmap_next、mlock、用户页表生命周期跟共享 mm 走。
- exec/exit 已做基础生命周期收口：exec 安装新 MmRef 并重置 trap slot，process-style CLONE_VM 子进程退出只释放自身 trap slot，不销毁父进程仍持有的 shared mm。
- 后续改进：把跨多步的 mmap/mremap/brk 修改进一步收敛到单次 mm lock/事务入口，补 glibc `clone08`/robust futex/exec-after-CLONE_VM 专项回归。
- 验证入口：`clone02`、`clone05`、pthread/robust futex、fork/COW 回归、exec/exit mm refcount 清理、本地 `clone_vm_mmap_smoke`。

### 第五阶段：替换线性 Vec VMA 查找（主体已推进）

- `VmRegionSet` 已成为 start-keyed tree-backed VMA 集合，`Vec<VmRegion>` 只保留为
  snapshot/rollback/procfs 输出等派生视图，不再作为权威结构。
- VMA containing、overlap、snapshot、共享映射扫描和 grow-down 精确候选已使用
  predecessor/range 查询；page fault lookup、mremap containment、brk overlap 等
  外层入口都通过 `VmRegionSet` 的语义 API。
- mmap layout policy 已有 top-down/bottom-up 搜索、stack guard gap、brk gap、
  `MAP_FIXED_NOREPLACE`、`MAP_GROWSDOWN`、arch 用户 VA 限制和 per-mm ASLR。
  每个 `MemorySet` 保存 page-aligned `mmap_aslr_offset`；fork/CLONE_VM 继承同一
  地址空间布局，exec/reset 重新生成。无 hint 的非 fixed mmap 从
  `USER_VA_TOP - offset` 开始 top-down 搜索，hint 和 `MAP_FIXED` 语义不变，
  失败后仍回到底部 fallback；`shmat(NULL)` 和 `mremap(MAYMOVE)` 的自动选址也
  复用同一个 mm-local placement 入口。
- 验证：本轮 per-mm ASLR 接入已跑 `cargo fmt`、riscv64 `os` debug/release
  cargo check、loongarch64 `os` cargo check；focused riscv64 debug memory smoke
  7 个探针全部通过，debug VM invariant 未触发。
- 第五阶段继续推进：free-area search 已去掉 `occupied_user_ranges_with_metadata()`
  的临时 Vec 构造。`find_free_mmap_range()` 现在通过正向/反向 merged occupied
  scanner 实时合并 concrete `MapArea` 和权威 `VmRegionSet` 两个有序来源，再在
  hole 内做 top-down/bottom-up placement 检查。语义上仍保留 grow-down guard、
  ASLR ceiling、hint 优先、`MAP_FIXED` 外部路径和 bottom-up fallback，但后续替换为
  gap tracking / interval tree 时改动面会集中在 scanner 内。反向 scanner 按
  range end 降序取下一个候选，保证重叠桥接区间会在 emit 前被合并，输出等价于
  normalized ranges 的反向遍历。
- 验证：本轮 gap scanner 改造已跑 `cargo fmt`、riscv64 `os` debug/release
  cargo check、loongarch64 `os` cargo check；focused riscv64 debug memory smoke
  7 个探针全部通过，debug VM invariant 未触发。
- 第五阶段继续推进：`VmRegionSet::replace_normalized()` 和旧的 Vec
  `normalize_vm_region_list()` 已删除。fork/clone 继承直接 clone tree-backed
  `VmRegionSet`；snapshot/rollback 恢复、普通 VMA 插入和 file-valid metadata 更新
  都走局部 `insert_merged()`/remove+reinsert，不再把整棵 VMA tree 物化成 Vec 后
  重建。`set_file_valid_by_identity()` 也改成局部 remove + update + merge，
  保留文件增长/缩小时与相邻 VMA 的合并机会。
- 验证：本轮 normalize removal 已跑 `cargo fmt`、riscv64 `os` debug/release
  cargo check、loongarch64 `os` cargo check；focused riscv64 debug memory smoke
  7 个探针全部通过，debug VM invariant 未触发。
- 验证入口：`mmapstress`、`mallocstress`、proc maps/smaps、mincore/mlock/madvise，以及长时间混合 mmap/munmap 压测。

### 第六阶段：page fault 从 VMA 元数据驱动

- 缺页处理应先查 VMA：访问权限、grow-down、COW/shared/private、file offset、SIGBUS tail、lazy allocation 都从 `VmRegion` 推导。
- `MapArea` 逐步退化为 resident page / frame tracker 缓存，不再作为 mmap policy 的第二来源。
- 这是消除 `map_type/map_perm` 双份权威的关键步骤。
- 验证入口：mmap/mprotect/COW/fork/page-fault 重压测试、stack growth、userfaultfd 后续测试，以及 debug invariant 全开回归。
