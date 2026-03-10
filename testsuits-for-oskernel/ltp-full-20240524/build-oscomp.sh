#!/bin/bash


# 对ltp的修改：
# 在testcases/kernel/syscalls/Makefile中添加 FILTER_OUT_DIRS += fmtmsg rt_sigtimedwait rt_tgsigqueueinfo timer_create
# 将所有的 #include <sys/sysinfo.h> 替换为 #include <linux/sysinfo.h>
# find . -type f \( -name "*.c" -o -name "*.h" \) -exec sed -i 's/#include <sys\/sysinfo.h>/#include <linux\/sysinfo.h>/g' {} \;

set -ex

make clean
make autotools

if [[ "$1" == *"riscv"* ]]; then
    if [[ "$1" == *"musl"* ]]; then
        ./configure  --prefix=/$1/ltp --host=riscv64-linux-musl CC=riscv64-linux-musl-gcc --with-target-cflags='-march=rv64gc' --without-tirpc
    else
        ./configure  --prefix=/$1/ltp --host=riscv64-linux-gnu CC=riscv64-linux-gnu-gcc --without-tirpc
    fi
else
    if [[ "$1" == *"musl"* ]]; then
        ./configure  --prefix=/$1/ltp --host=loongarch64-linux-musl CC=loongarch64-linux-musl-gcc --without-tirpc
    else
        ./configure  --prefix=/$1/ltp --host=loongarch64-linux-gnu CC=loongarch64-linux-gnu-gcc --without-tirpc
    fi
fi

make V=1 -j  && make install DESTDIR=/