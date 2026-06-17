
export LANG=C.UTF-8
export LC_ALL=C.UTF-8
export LOCPATH=/usr/lib/locale

./busybox echo "#### OS COMP TEST GROUP START libctest ####"
./run-static.sh
./run-dynamic.sh
./busybox echo "#### OS COMP TEST GROUP END libctest ####"
