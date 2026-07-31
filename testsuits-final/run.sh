#!/usr/bin/env bash
# bash is more compatible

set -Eeuo pipefail

# get absolute paht  where we run this script. -- is for edge case when "-" starts at the folder
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# workspace place
WORKSPACE_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
# os source
OS_DIR="${WORKSPACE_DIR}/os"
# source code of the final img
SOURCE_DIR="${SCRIPT_DIR}/testsuits-for-oskernel"
# where we run (in tmp file )
RUN_ROOT="${FINAL_RUN_ROOT:-${SCRIPT_DIR}/.tmp/final-runs}"

usage() {
    cat <<'EOF'
Usage:
  ./run.sh [shell|cagent|buildstorm]

Environment:
  ARCH=riscv64|loongarch64  Guest architecture (default: riscv64)
  IMAGE_MODE=snapshot|copy  Image reset strategy (default: snapshot)
  MEM=<qemu-size>           Guest memory (default: 8G)
  SMP=<count>               Guest CPUs (default: 8 for RISC-V, 12 for LoongArch)
  LOG=<level>               Kernel log level passed to make (default: warn)
  BOOT_TIMEOUT=<seconds>    Automated-mode boot timeout (default: 1800)
  TEST_TIMEOUT=<seconds>    Automated test timeout (default: 300 for CAgent,
                            18000 for BuildStorm)
  FINAL_RUN_ROOT=<path>     Runtime images and logs directory

Examples:
  ARCH=riscv64 ./run.sh shell
  ARCH=loongarch64 ./run.sh cagent
  ARCH=loongarch64 IMAGE_MODE=copy ./run.sh buildstorm

Image modes:
  snapshot  Run with QEMU -snapshot. All guest writes are discarded on exit.
  copy      Recreate a writable raw working copy before the run. The latest
            copy is retained for diagnosis and replaced by the next copy run.
EOF
}

# exit with message
die() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

# print something in log and terminal
note() {
    printf '%s\n' "$*" | tee -a "${LOG_FILE}"
}

require_command() {
    command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

sha256_file() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        die "neither shasum nor sha256sum is available"
    fi
}

# used for checking SMP
validate_positive_integer() {
    local name="$1"
    local value="$2"
    case "${value}" in
        ''|*[!0-9]*|0)
            die "${name} must be a positive integer, got '${value}'"
            ;;
    esac
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

if (( $# > 1 )); then
    usage >&2
    exit 2
fi

# Where main part starts
MODE="${1:-shell}"
ARCH="${ARCH:-riscv64}"
IMAGE_MODE="${IMAGE_MODE:-snapshot}"
MEM="${MEM:-8G}"
LOG_LEVEL="${LOG:-warn}"
BOOT_TIMEOUT="${BOOT_TIMEOUT:-1800}"

case "${MODE}" in
    shell|cagent|buildstorm) ;;
    *)
        usage >&2
        die "unsupported mode '${MODE}'"
        ;;
esac

case "${IMAGE_MODE}" in
    snapshot|copy) ;;
    *)
        usage >&2
        die "unsupported IMAGE_MODE '${IMAGE_MODE}'"
        ;;
esac

case "${ARCH}" in
    riscv64)
        FINAL_IMAGE="${SCRIPT_DIR}/sdcard-rv-pub.img"
        EXPECTED_SHA256="d899fe43d333d1d17ad8a5f8a8b74b68117b8c1ceacfc3843bfeadb1ca705bd1"
        QEMU_BIN="qemu-system-riscv64"
        DEFAULT_SMP=8
        ;;
    loongarch64)
        FINAL_IMAGE="${SCRIPT_DIR}/sdcard-la-pub.img"
        EXPECTED_SHA256="2ad9d955684297abe9db48d94f1b7fcc488268fc8f481408c55b1ec27f520c6a"
        QEMU_BIN="qemu-system-loongarch64"
        DEFAULT_SMP=12
        ;;
    *)
        usage >&2
        die "unsupported ARCH '${ARCH}'"
        ;;
esac

SMP="${SMP:-${DEFAULT_SMP}}"
validate_positive_integer "SMP" "${SMP}"
validate_positive_integer "BOOT_TIMEOUT" "${BOOT_TIMEOUT}"

if [[ -z "${TEST_TIMEOUT:-}" ]]; then
    if [[ "${MODE}" == "buildstorm" ]]; then
        TEST_TIMEOUT=18000
    else
        TEST_TIMEOUT=300
    fi
fi
validate_positive_integer "TEST_TIMEOUT" "${TEST_TIMEOUT}"

require_command make
require_command cargo
require_command file
require_command git
require_command "${QEMU_BIN}"
if [[ "${IMAGE_MODE}" == "copy" ]]; then
    require_command qemu-img
fi
if [[ "${MODE}" != "shell" ]]; then
    require_command expect
    require_command python3
fi

[[ -d "${OS_DIR}" ]] || die "kernel directory not found: ${OS_DIR}"
[[ -f "${FINAL_IMAGE}" ]] || die "final image not found: ${FINAL_IMAGE}"
[[ -d "${SOURCE_DIR}/.git" ]] || die "final source checkout not found: ${SOURCE_DIR}"

SOURCE_BRANCH="$(git -C "${SOURCE_DIR}" rev-parse --abbrev-ref HEAD)"
[[ "${SOURCE_BRANCH}" == "final-2026" ]] ||
    die "final source must be on branch final-2026, got '${SOURCE_BRANCH}'"
SOURCE_COMMIT="$(git -C "${SOURCE_DIR}" rev-parse HEAD)"

if [[ "${MODE}" == "cagent" ]]; then
    [[ -f "${SOURCE_DIR}/judge/judge_cagent-glibc.py" ]] ||
        die "CAgent judge not found in final source checkout"
elif [[ "${MODE}" == "buildstorm" ]]; then
    [[ -f "${SOURCE_DIR}/judge/judge_buildstorm-glibc.py" ]] ||
        die "BuildStorm judge not found in final source checkout"
fi

IMAGE_DESCRIPTION="$(file "${FINAL_IMAGE}")"
[[ "${IMAGE_DESCRIPTION}" == *"ext4 filesystem data"* ]] ||
    die "final image is not recognized as ext4: ${IMAGE_DESCRIPTION}"

TIMESTAMP="$(date '+%Y%m%d-%H%M%S')"
RUN_DIR="${RUN_ROOT}/${TIMESTAMP}-${ARCH}-${MODE}"
LOG_FILE="${RUN_DIR}/serial.log"
SCORE_FILE="${RUN_DIR}/score.json"
JUDGE_LOG="${RUN_DIR}/judge.log"
mkdir -p "${RUN_DIR}"

note "Final test run"
note "  mode:          ${MODE}"
note "  architecture:  ${ARCH}"
note "  image mode:    ${IMAGE_MODE}"
note "  memory/SMP:    ${MEM}/${SMP}"
note "  source commit: ${SOURCE_COMMIT}"
note "  reference:     ${FINAL_IMAGE}"
note "  log:           ${LOG_FILE}"
note "Verifying reference image SHA-256 (this scans the 14 GiB image)..."

ACTUAL_SHA256="$(sha256_file "${FINAL_IMAGE}")"
[[ "${ACTUAL_SHA256}" == "${EXPECTED_SHA256}" ]] ||
    die "image checksum mismatch: expected ${EXPECTED_SHA256}, got ${ACTUAL_SHA256}"
note "Image checksum verified: ${ACTUAL_SHA256}"

if [[ -n "$(git -C "${SOURCE_DIR}" status --porcelain)" ]]; then
    note "warning: final source checkout has local modifications"
fi

# Homebrew keeps e2fsprogs keg-only. Add its sbin directory when available so
# ext4-fs-packer can find debugfs while rebuilding the small /user boot disk.
if ! command -v debugfs >/dev/null 2>&1 && command -v brew >/dev/null 2>&1; then
    if E2FSPROGS_PREFIX="$(brew --prefix e2fsprogs 2>/dev/null)"; then
        E2FSPROGS_SBIN="${E2FSPROGS_PREFIX}/sbin"
        if [[ -x "${E2FSPROGS_SBIN}/debugfs" ]]; then
            PATH="${E2FSPROGS_SBIN}:${PATH}"
            export PATH
        fi
    fi
fi

QEMU_EXTRA_ARGS=""
EXT4_REBUILD_VALUE="${EXT4_REBUILD:-0}"
RUN_IMAGE="${FINAL_IMAGE}"

if [[ "${IMAGE_MODE}" == "snapshot" ]]; then
    QEMU_EXTRA_ARGS="-snapshot"
else
    WORKING_IMAGE_DIR="${RUN_ROOT}/images"
    WORKING_IMAGE="${WORKING_IMAGE_DIR}/sdcard-${ARCH}-working.img"
    mkdir -p "${WORKING_IMAGE_DIR}"

    IMAGE_SIZE_BYTES="$(wc -c < "${FINAL_IMAGE}" | tr -d '[:space:]')"
    AVAILABLE_KB="$(df -Pk "${WORKING_IMAGE_DIR}" | awk 'NR == 2 {print $4}')"
    REQUIRED_KB="$((IMAGE_SIZE_BYTES / 1024 + 1024 * 1024))"
    if [[ -n "${AVAILABLE_KB}" ]] && (( AVAILABLE_KB < REQUIRED_KB )); then
        die "copy mode needs at least ${REQUIRED_KB} KiB free, only ${AVAILABLE_KB} KiB available"
    fi

    note "Creating fresh writable image copy: ${WORKING_IMAGE}"
    rm -f -- "${WORKING_IMAGE}"
    qemu-img convert -p -f raw -O raw "${FINAL_IMAGE}" "${WORKING_IMAGE}" \
        2>&1 | tee -a "${LOG_FILE}"
    RUN_IMAGE="${WORKING_IMAGE}"

    # Copy mode is intended for a clean benchmark state. Rebuild the /user boot
    # filesystem as well so an earlier writable run cannot leak state into it.
    EXT4_REBUILD_VALUE=1
fi

MAKE_COMMAND=(
    make
    -C "${OS_DIR}"
    run_final
    "ARCH=${ARCH}"
    "SUBMIT=0"
    "BASH_SHELL=1"
    "LOG=${LOG_LEVEL}"
    "SMP=${SMP}"
    "MEM=${MEM}"
    "EXT4_REBUILD=${EXT4_REBUILD_VALUE}"
    "USER_EXT4_SIZE=${USER_EXT4_SIZE:-256M}"
    "FINAL_IMG=${RUN_IMAGE}"
    "QEMU_TIMEOUT=0"
    "QEMU_EXTRA_ARGS=${QEMU_EXTRA_ARGS}"
)

note "Launching QEMU; final image is /dev/vda and the generated /user image is /dev/vdb."

run_interactive() {
    local status
    set +e
    "${MAKE_COMMAND[@]}" 2>&1 | tee -a "${LOG_FILE}"
    status=${PIPESTATUS[0]}
    set -e
    return "${status}"
}

run_automated() {
    local marker
    local guest_command

    case "${MODE}" in
        cagent)
            marker="#### OS COMP TEST GROUP END cagent ####"
            guest_command="./cagent_testcode.sh"
            ;;
        buildstorm)
            marker="#### OS COMP TEST GROUP END buildstorm ####"
            guest_command="./buildstorm_testcode.sh"
            ;;
        *)
            die "internal error: automated runner called for ${MODE}"
            ;;
    esac

    expect -f - \
        "${LOG_FILE}" \
        "${BOOT_TIMEOUT}" \
        "${TEST_TIMEOUT}" \
        "${marker}" \
        "${guest_command}" \
        "${MAKE_COMMAND[@]}" <<'EXPECT_EOF'
set log_path [lindex $argv 0]
set boot_timeout [lindex $argv 1]
set test_timeout [lindex $argv 2]
set end_marker [lindex $argv 3]
set guest_command [lindex $argv 4]
set make_command [lrange $argv 5 end]

log_file -a $log_path
set timeout $boot_timeout
spawn -noecho {*}$make_command

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

send -- "cd /glibc\r"
expect {
    -re {CongCore:.*\$ } {}
    timeout {
        puts stderr "error: timed out entering /glibc"
        stop_qemu
        expect eof
        exit 124
    }
    eof {
        set result [wait]
        set status [lindex $result 3]
        puts stderr "error: QEMU exited while entering /glibc"
        if {$status == 0} { set status 125 }
        exit $status
    }
}

set timeout $test_timeout
send -- "$guest_command\r"
expect {
    -exact $end_marker {
        stop_qemu
    }
    -re {CongCore:.*\$ } {
        puts stderr "error: final test returned to the shell without its completion marker"
        stop_qemu
        expect eof
        exit 126
    }
    timeout {
        puts stderr "error: timed out waiting for final test completion"
        stop_qemu
        expect eof
        exit 124
    }
    eof {
        set result [wait]
        set status [lindex $result 3]
        puts stderr "error: QEMU exited before the final test completed"
        if {$status == 0} { set status 125 }
        exit $status
    }
}

set timeout 30
expect {
    eof {}
    timeout {
        puts stderr "error: QEMU did not exit after the test completed"
        exit 124
    }
}
set result [wait]
exit [lindex $result 3]
EXPECT_EOF
}

score_automated_run() {
    if [[ "${MODE}" == "cagent" ]]; then
        python3 "${SOURCE_DIR}/judge/judge_cagent-glibc.py" \
            < "${LOG_FILE}" > "${SCORE_FILE}" 2> "${JUDGE_LOG}"
    else
        python3 "${SOURCE_DIR}/judge/judge_buildstorm-glibc.py" \
            "${LOG_FILE}" > "${SCORE_FILE}" 2> "${JUDGE_LOG}"
    fi

    cat "${SCORE_FILE}"
    if [[ -s "${JUDGE_LOG}" ]]; then
        cat "${JUDGE_LOG}" >&2
    fi

    python3 - "${MODE}" "${SCORE_FILE}" <<'PY'
import json
import sys

mode, score_path = sys.argv[1:]
with open(score_path, encoding="utf-8") as score_file:
    results = json.load(score_file)

if mode == "cagent":
    ok = len(results) == 10 and all(item.get("pass") == 1 for item in results)
    if not ok:
        print("error: one or more CAgent cases failed", file=sys.stderr)
        raise SystemExit(1)
else:
    required = {
        "buildstorm env toolchain",
        "buildstorm env minibuild",
        "buildstorm compile ok",
    }
    passed = {
        item.get("name")
        for item in results
        if item.get("pass") == 1
    }
    if not required.issubset(passed):
        print("error: BuildStorm correctness requirements failed", file=sys.stderr)
        raise SystemExit(1)
PY
}

if [[ "${MODE}" == "shell" ]]; then
    run_interactive
else
    run_status=0
    score_status=0

    run_automated || run_status=$?
    score_automated_run || score_status=$?

    if ((run_status != 0)); then
        exit "${run_status}"
    fi
    if ((score_status != 0)); then
        exit "${score_status}"
    fi
fi

note "Run completed successfully."
if [[ "${IMAGE_MODE}" == "copy" ]]; then
    note "Writable working image retained at: ${RUN_IMAGE}"
fi
note "Artifacts: ${RUN_DIR}"
