#!/bin/bash
# compile_c.sh - 在 /mnt 下查找所有 .c 文件并用 riscv64 交叉编译器编译
# 在 Docker 容器内运行: docker run -it --rm -v /opt/riscv:/opt/riscv -v /mnt:/mnt riscv64-musl-cross

set -e

CC="/opt/riscv/bin/riscv64-unknown-linux-gnu-gcc"
MNT="/mnt"

if [ ! -x "$CC" ]; then
    echo "❌ 交叉编译器不存在: $CC"
    exit 1
fi

success=0
fail=0

echo "🔍 在 $MNT 下查找 .c 文件..."

while IFS= read -r -d '' cfile; do
    outfile="${cfile%.c}.o"
    echo "🔨 $cfile -> $outfile"
    if "$CC" -c "$cfile" -o "$outfile" 2>&1; then
        success=$((success + 1))
        echo "   ✅"
    else
        fail=$((fail + 1))
        echo "   ❌"
    fi
done < <(find "$MNT" -name "*.c" -type f -print0)

echo ""
echo "📊 完成: 成功=$success, 失败=$fail"
