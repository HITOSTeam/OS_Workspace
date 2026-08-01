#!/usr/bin/env bash

set -Eeuo pipefail

if (( $# != 3 )); then
    echo "usage: $0 <workspace> <iozone-root-image> <log-path>" >&2
    exit 2
fi

workspace=$1
root_image=$2
log_path=$3
shared_workspace=${IOZONE_SHARED_WORKSPACE:-/Users/bytedance/projects/OS_Workspace}
user_image=${IOZONE_USER_IMG:-"${shared_workspace}/ext4-fs-packer/target/user.ext4"}

for path in \
    "${workspace}/os" \
    "${root_image}" \
    "${user_image}"
do
    if [[ ! -e "${path}" ]]; then
        echo "error: required path not found: ${path}" >&2
        exit 2
    fi
done
if ! command -v expect >/dev/null 2>&1; then
    echo "error: expect is required" >&2
    exit 127
fi

mkdir -p "$(dirname "${log_path}")"

{
    echo "IOZone focused run"
    echo "  workspace:       ${workspace}"
    echo "  kernel commit:   $(git -C "${workspace}/os" rev-parse HEAD)"
    echo "  root image:      ${root_image}"
    echo "  user image:      ${user_image}"
    echo "  architecture:    riscv64"
    echo "  memory/SMP:      2G/8"
    echo "  image mode:      snapshot"
    echo "  workload:        3x {4-worker sequential write/read + random read}"
} >"${log_path}"

build_command=(
    make
    -C "${workspace}/os"
    kernel
    ARCH=riscv64
    SUBMIT=0
    BASH_SHELL=0
    LOG=warn
)

if [[ "${IOZONE_SKIP_BUILD:-0}" != "1" ]]; then
    set +e
    "${build_command[@]}" 2>&1 | tee -a "${log_path}"
    build_status=${PIPESTATUS[0]}
    set -e
    if (( build_status != 0 )); then
        exit "${build_status}"
    fi
fi

kernel_elf="${workspace}/target/riscv64gc-unknown-none-elf/release/os"
if [[ ! -f "${kernel_elf}" ]]; then
    echo "error: kernel ELF not found: ${kernel_elf}" >&2
    exit 2
fi

qemu_command=(
    qemu-system-riscv64
    -machine virt
    -kernel "${kernel_elf}"
    -m 2G
    -smp 8
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

if [[ -n "${IOZONE_MONITOR_SOCKET:-}" ]]; then
    qemu_command+=(
        -monitor
        "unix:${IOZONE_MONITOR_SOCKET},server=on,wait=off"
    )
fi

expect -f - "${log_path}" "${qemu_command[@]}" <<'EXPECT_EOF'
set log_path [lindex $argv 0]
set qemu_command [lrange $argv 1 end]

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

# These are the official IOZone suite's 4-worker sequential and random-read
# commands, with 4 MiB per worker to make lock and block-queue contention
# measurable while keeping the focused regression short. Repeating three
# times exposes both cold and warm-cache behavior without invoking any of the
# unrelated unixbench/libcbench/BuildStorm workloads.
set guest_command {/glibc/busybox sh -c 'cd /glibc || exit 1; export LD_LIBRARY_PATH=/glibc/lib; RC=0; N=1; while [ "$N" -le 3 ]; do read START _ < /proc/uptime; ./iozone -t 4 -i 0 -i 1 -r 1k -s 4m; SEQ_RC=$?; ./iozone -t 4 -i 0 -i 2 -r 1k -s 4m; RAND_RC=$?; read END _ < /proc/uptime; echo "IOZONE_FOCUSED_RUN run=$N start_s=$START end_s=$END seq_rc=$SEQ_RC rand_rc=$RAND_RC"; if [ "$SEQ_RC" -ne 0 ] || [ "$RAND_RC" -ne 0 ]; then RC=1; fi; N=$((N + 1)); done; ./busybox cat /proc/perf; echo "IOZONE_FOCUSED_DONE rc=$RC"'}

set timeout 900
send -- "$guest_command\r"
expect {
    -re {[\r\n]+IOZONE_FOCUSED_DONE rc=([0-9]+)[\r\n]+} {
        set guest_status $expect_out(1,string)
        stop_qemu
    }
    timeout {
        puts stderr "error: focused IOZone workload timed out"
        stop_qemu
        expect eof
        exit 124
    }
    eof {
        set result [wait]
        set status [lindex $result 3]
        puts stderr "error: QEMU exited before focused IOZone completion"
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
