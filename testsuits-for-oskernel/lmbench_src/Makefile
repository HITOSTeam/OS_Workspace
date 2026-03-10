# Makefile for top level of lmbench
# $Id: Makefile 1.17 00/05/31 16:16:15+03:00 staelin@hpli8.hpli.hpl.hp.com $

# Possible things to $(MAKE):
#
# build		(default) go to the source directory and build the benchmark
# results	go to the source directory and build and run the benchmark
# rerun		run the benchmark again
# see		see the results that came with this release
#		Go to the results directory and read the Makefile.
# doc.lpr	print the documentation
# doc.x		preview the documentation (needs X, groff, pic, etc)
# clean		go to the subdirs and $(MAKE) clean
# get		$(MAKE) sure all files are checked out
# shar		build a shippable shar archive
#CC=riscv64-linux-gcc
#OS=riscv64-linux-gnu

SHELL=/bin/sh
HOST=$(patsubst %-gcc,%,$(CC))

build:
	mkdir -p /opt/loongarch64-linux-musl-cross/loongarch64-linux-musl/usr/include/
	cp -fr /opt/toolchain-loongarch64-linux-gnu-gcc8-host-x86_64-2022-07-18/sysroot/usr/include/sys /opt/loongarch64-linux-musl-cross/loongarch64-linux-musl/usr/include/
	rm -fr libtirpc-1.3.6
	tar zxf libtirpc-1.3.6.tar.gz
	cd libtirpc-1.3.6 && ./configure --host=${HOST} -disable-shared --disable-gssapi && make -j 8
	cd ..
	cd src && $(MAKE)

oscomp:
	cd src && $(MAKE) oscomp

results: FRC
	cd src && $(MAKE) results

rerun: 
	cd src && $(MAKE) rerun

see:
	cd results && $(MAKE) summary percent 2>/dev/null | more

doc.lpr:
	cd doc && $(MAKE) PS && lpr *.PS

doc.x:
	cd doc && $(MAKE) x

clobber clean: 
	for i in doc src results scripts; do \
		echo ===== $$i =====; \
		(cd $$i && $(MAKE) clean); \
	done
	/bin/rm -rf bin/*
	-bk clean 

get: 
	for i in doc src results scripts; do \
		echo ===== $$i =====; \
		(cd $$i && bk get -q); \
	done
	@co -q

info: 
	for i in doc src results scripts; do \
		echo ===== $$i =====; \
		(cd $$i && info); \
	done

release: scripts/mkrelease
	scripts/mkrelease

scripts/mkrelease:
	cd scripts && co mkrelease

# XXX - . must be named lmbench for this to work
shar:
	$(MAKE) clean
	co -q Makefile
	$(MAKE) get
	cd .. && \
	find lmbench -type f -print  | egrep -v 'noship|RCS' > /tmp/FILES
	cd .. && shar -S -a -n lmbench1.0 -L 50K < /tmp/FILES 

FRC:
