#!/bin/bash

# If the script was invoked with sh, re-exec under bash to support arrays
if [ -z "$BASH" ] || [ -z "$BASH_VERSION" ]; then
    exec /bin/bash "$0" "$@"
fi

echo "#### OS COMP TEST GROUP START ltp-musl ####"

# 定义目标目录
target_dir="/musl/ltp/testcases/bin"

# 列出要排除的测试名（只写 basename，不带路径）
exclude_names=(
    # 在这里添加不想执行的测试用例名，例如：
    # "futex01"
    # "hang_test"
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

echo "#### OS COMP TEST GROUP END ltp-musl ####"