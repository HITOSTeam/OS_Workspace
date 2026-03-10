
#!/bin/bash

# If the script was invoked with sh, re-exec under bash to support arrays
if [ -z "$BASH" ] || [ -z "$BASH_VERSION" ]; then
    exec /bin/bash "$0" "$@"
fi

echo "#### OS COMP TEST GROUP START ltp-glibc ####"

target_dir="/glibc/ltp/testcases/bin"

exclude_names=(

    "cgroup_core01"
    "cgroup_core02"
    "cgroup_core03"
    "cgroup_fj_common.sh"
    "cgroup_fj_function.sh"
    "cgroup_fj_proc"
    "cgroup_fj_stress.sh"
    # wrap it by ""
    "cgroup_lib.sh"
    "cgroup_regression_3_1.sh"
    "cgroup_regression_3_2.sh"
    "cgroup_regression_5_1.sh"
    "cgroup_regression_5_2.sh"
    "cgroup_regression_6_1.sh"
    "cgroup_regression_6_2.sh"
    "cgroup_regression_fork_processes"
    "cgroup_regression_getdelays"
    "cgroup_regression_test.sh"
    "cgroup_xattr"
)

is_excluded() {
    local name="$1"
    for e in "${exclude_names[@]}"; do
        [ "$e" = "$name" ] && return 0
    done
    return 1
}

if [ ! -d "$target_dir" ]; then
    echo "WARN: target dir not found: $target_dir"
else
    for f in "$target_dir"/*; do
        [ -f "$f" ] || continue
        name=$(basename "$f")
        if is_excluded "$name"; then
            echo "SKIP LTP CASE $name (excluded)"
            continue
        fi
        if [ ! -x "$f" ]; then
            echo "WARN: 测试用例文件不可执行 - $f"
            continue
        fi

        echo "RUN LTP CASE $name"
        "$f"
        ret=$?
        if [ $ret -eq 0 ]; then
            echo "PASS LTP CASE $name : $ret"
        else
            echo "FAIL LTP CASE $name : $ret"
        fi
    done
fi

echo "#### OS COMP TEST GROUP END ltp-glibc ####"
