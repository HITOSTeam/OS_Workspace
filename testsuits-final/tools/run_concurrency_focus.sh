#!/usr/bin/env bash

set -Eeuo pipefail

if (( $# != 3 )); then
    echo "usage: $0 <workspace> <root-image> <log-path>" >&2
    exit 2
fi

workspace=$1
root_image=$2
log_path=$3
shared_workspace=${CONCURRENCY_SHARED_WORKSPACE:-/Users/bytedance/projects/OS_Workspace}
user_image=${CONCURRENCY_USER_IMG:-"${shared_workspace}/ext4-fs-packer/target/user.ext4"}
sample_count=${CONCURRENCY_SAMPLES:-5}
phase=${CONCURRENCY_PHASE:-all}
label=${CONCURRENCY_LABEL:-unnamed}
memory=${CONCURRENCY_MEM:-2G}
smp=${CONCURRENCY_SMP:-8}
target_dir=${CONCURRENCY_TARGET_DIR:-"${workspace}/target"}

case ${sample_count} in
    ''|*[!0-9]*|0)
        echo "error: CONCURRENCY_SAMPLES must be a positive integer" >&2
        exit 2
        ;;
esac
case ${smp} in
    ''|*[!0-9]*|0)
        echo "error: CONCURRENCY_SMP must be a positive integer" >&2
        exit 2
        ;;
esac
case ${phase} in
    all)
        run_benchmark=1
        run_regression=1
        ;;
    benchmark)
        run_benchmark=1
        run_regression=0
        ;;
    regression)
        run_benchmark=0
        run_regression=1
        ;;
    *)
        echo "error: CONCURRENCY_PHASE must be all, benchmark, or regression" >&2
        exit 2
        ;;
esac

for path in \
    "${workspace}/os" \
    "${root_image}" \
    "${user_image}"
do
    if [[ ! -e ${path} ]]; then
        echo "error: required path not found: ${path}" >&2
        exit 2
    fi
done
for command in cargo expect git qemu-system-riscv64; do
    if ! command -v "${command}" >/dev/null 2>&1; then
        echo "error: required command not found: ${command}" >&2
        exit 127
    fi
done

mkdir -p "$(dirname "${log_path}")" "${target_dir}"

diff_hash=clean
if [[ -n $(git -C "${workspace}/os" status --porcelain) ]]; then
    if command -v shasum >/dev/null 2>&1; then
        diff_hash=$(git -C "${workspace}/os" diff --binary | shasum -a 256 | awk '{print $1}')
    else
        diff_hash=$(git -C "${workspace}/os" diff --binary | sha256sum | awk '{print $1}')
    fi
fi

{
    echo "Linux concurrency focused run"
    echo "  label:           ${label}"
    echo "  workspace:       ${workspace}"
    echo "  kernel commit:   $(git -C "${workspace}/os" rev-parse HEAD)"
    echo "  kernel diff:     ${diff_hash}"
    echo "  root image:      ${root_image}"
    echo "  root image size: $(wc -c <"${root_image}" | tr -d '[:space:]')"
    echo "  user image:      ${user_image}"
    echo "  QEMU:            $(qemu-system-riscv64 --version | head -n 1)"
    echo "  architecture:    riscv64"
    echo "  memory/SMP:      ${memory}/${smp}"
    echo "  image mode:      snapshot"
    echo "  samples:         ${sample_count}"
    echo "  phase:           ${phase}"
    echo "  workload:        hackbench 4 modes and lat_proc fork (benchmark); 400-task process stress and fork/close LTP (regression)"
} >"${log_path}"

if [[ ${CONCURRENCY_SKIP_BUILD:-0} != 1 ]]; then
    build_command=(
        cargo build
        --offline
        --release
        --manifest-path "${workspace}/os/Cargo.toml"
        --target riscv64gc-unknown-none-elf
    )
    set +e
    CARGO_TARGET_DIR="${target_dir}" \
        RUSTFLAGS=${CONCURRENCY_RUSTFLAGS:--Cforce-frame-pointers=yes} \
        TMPDIR=${CONCURRENCY_TMPDIR:-"${shared_workspace}/testsuits-final/.tmp"} \
        "${build_command[@]}" 2>&1 | tee -a "${log_path}"
    build_status=${PIPESTATUS[0]}
    set -e
    if (( build_status != 0 )); then
        exit "${build_status}"
    fi
fi

kernel_elf="${target_dir}/riscv64gc-unknown-none-elf/release/os"
if [[ ! -f ${kernel_elf} ]]; then
    echo "error: kernel ELF not found: ${kernel_elf}" >&2
    exit 2
fi

qemu_command=(
    qemu-system-riscv64
    -machine virt
    -kernel "${kernel_elf}"
    -m "${memory}"
    -smp "${smp}"
    -nographic
    -rtc base=utc
    -no-reboot
    -bios default
    -drive "file=${root_image},if=none,format=raw,id=x0"
    -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0
    -drive "file=${user_image},if=none,format=raw,id=x1"
    -device virtio-blk-device,drive=x1,bus=virtio-mmio-bus.1
    -device virtio-net-device,netdev=net
    -netdev user,id=net
    -snapshot
)

expect -f - "${log_path}" "${sample_count}" "${run_benchmark}" "${run_regression}" "${qemu_command[@]}" <<'EXPECT_EOF'
set log_path [lindex $argv 0]
set sample_count [lindex $argv 1]
set run_benchmark [lindex $argv 2]
set run_regression [lindex $argv 3]
set qemu_command [lrange $argv 4 end]

log_file -a $log_path
set timeout 1800
spawn -noecho {*}$qemu_command

proc stop_qemu {} {
    send -- "\001x"
}

expect {
    -re {CongCore:.*\$ } {}
    timeout {
        puts stderr "error: timed out waiting for CongCore shell"
        stop_qemu
        expect eof
        exit 124
    }
    eof {
        set result [wait]
        set status [lindex $result 3]
        puts stderr "error: QEMU exited before the CongCore shell appeared"
        if {$status == 0} { set status 125 }
        exit $status
    }
}

set guest_command "/glibc/busybox sh -c 'cd /glibc || exit 1; export LD_LIBRARY_PATH=/glibc/lib; SAMPLES=$sample_count; RUN_BENCHMARK=$run_benchmark; RUN_REGRESSION=$run_regression; RC=0; if \[ \"\$RUN_BENCHMARK\" -eq 1 \]; then run_hackbench() { LABEL=\"\$1\"; shift; N=1; while \[ \"\$N\" -le \"\$SAMPLES\" \]; do read START REST < /proc/uptime; ./hackbench \"\$@\" -g 10 -f 20 -l 100; STATUS=\$?; read END REST < /proc/uptime; echo \"CONCURRENCY_METRIC workload=\$LABEL sample=\$N start_s=\$START end_s=\$END rc=\$STATUS\"; if \[ \"\$STATUS\" -ne 0 \]; then RC=1; fi; N=\$((N + 1)); done; }; run_hackbench hb_process_socket -P; run_hackbench hb_process_pipe -P -p; run_hackbench hb_thread_socket -T; run_hackbench hb_thread_pipe -T -p; N=1; while \[ \"\$N\" -le \"\$SAMPLES\" \]; do read START REST < /proc/uptime; echo \"LAT_PROC_BEGIN sample=\$N\"; ./lmbench_all lat_proc -P 8 -W 1 -N 5 fork; STATUS=\$?; read END REST < /proc/uptime; echo \"CONCURRENCY_METRIC workload=lat_proc_fork sample=\$N start_s=\$START end_s=\$END rc=\$STATUS\"; if \[ \"\$STATUS\" -ne 0 \]; then RC=1; fi; N=\$((N + 1)); done; fi; if \[ \"\$RUN_REGRESSION\" -eq 1 \]; then read START REST < /proc/uptime; ./hackbench -P -g 10 -f 20 -l 200; STATUS=\$?; read END REST < /proc/uptime; echo \"CONCURRENCY_STRESS tasks=400 start_s=\$START end_s=\$END rc=\$STATUS\"; if \[ \"\$STATUS\" -ne 0 \]; then RC=1; fi; export LTPROOT=/glibc/ltp; export PATH=\$LTPROOT/testcases/bin:\$PATH; export TMPDIR=/tmp; /glibc/busybox mkdir -p /tmp || exit 1; cd /tmp || exit 1; for TEST in fork03 fork04 fork05 fork07 fork08 fork09 fork10 close01 close02 close_range02; do \"\$LTPROOT/testcases/bin/\$TEST\"; STATUS=\$?; echo \"CONCURRENCY_LTP test=\$TEST rc=\$STATUS\"; if \[ \"\$STATUS\" -ne 0 \]; then RC=1; fi; done; fi; /glibc/busybox cat /proc/perf; echo \"CONCURRENCY_FOCUSED_DONE rc=\$RC\"'"

set timeout 3600
send -- "$guest_command\r"
expect {
    -re {[\r\n]+CONCURRENCY_FOCUSED_DONE rc=([0-9]+)[\r\n]+} {
        set guest_status $expect_out(1,string)
        stop_qemu
    }
    timeout {
        puts stderr "error: concurrency workload timed out"
        stop_qemu
        expect eof
        exit 124
    }
    eof {
        set result [wait]
        set status [lindex $result 3]
        puts stderr "error: QEMU exited before concurrency workload completion"
        if {$status == 0} { set status 125 }
        exit $status
    }
}

set timeout 30
expect {
    eof {}
    timeout {
        puts stderr "error: QEMU did not exit"
        exit 124
    }
}
set result [wait]
set qemu_status [lindex $result 3]
if {$qemu_status != 0} {
    exit $qemu_status
}
exit $guest_status
EXPECT_EOF
