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
