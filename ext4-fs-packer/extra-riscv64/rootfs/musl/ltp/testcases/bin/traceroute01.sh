#!/bin/sh
# SPDX-License-Identifier: GPL-2.0-or-later
# Copyright (c) 2019 Petr Vorel <pvorel@suse.cz>
# Copyright (c) 2017 Oracle and/or its affiliates. All Rights Reserved.
# Copyright (c) International Business Machines  Corp., 2001

TST_CNT=2
TST_NEEDS_CMDS="traceroute"
TST_SETUP="setup"
TST_TESTFUNC="test"
TST_NEEDS_TMPDIR=1

setup()
{

	TRACEROUTE=traceroute${TST_IPV6}
	tst_require_cmds $TRACEROUTE

	tst_res TINFO "$TRACEROUTE version:"
	tst_res TINFO $($TRACEROUTE -V 2>&1)
}

run_trace()
{
	local opts="$@"
	local ip=$(tst_ipaddr rhost)
	local pattern="^[ ]+1[ ]+$ip([ ]+[0-9]+[.][0-9]+ ms){3}"

	if $TRACEROUTE $opts 2>&1 | grep -q "invalid option"; then
		tst_res TCONF "$opts flag not supported"
		return
	fi

	# According to man pages, default sizes:
	local bytes=60
	[ "$TST_IPV6" ] && bytes=80
	local bytes_re="$bytes byte"

	# Modern traceroute reports the real TCP SYN packet size (44 bytes on
	# IPv4) even when the legacy packet-length argument is present.
	case " $opts " in
	*" -T "*) [ -z "$TST_IPV6" ] && bytes_re="(44|$bytes) byte";;
	esac

	EXPECT_PASS $TRACEROUTE $ip $bytes -n -m 2 $opts \>out.log

	grep -qE "$bytes_re" out.log
	if [ $? -ne 0 ]; then
		cat out.log
		tst_res TFAIL "packet size marker '$bytes_re' not found"
	else
		tst_res TPASS "$TRACEROUTE reports expected packet size"
	fi

	tail -1 out.log | grep -qE "$pattern"
	if [ $? -ne 0 ]; then
		cat out.log
		tst_res TFAIL "pattern '$pattern' not found in log"
	else
		tst_res TPASS "$TRACEROUTE test completed with 1 hop"
	fi
}

test1()
{
	tst_res TINFO "run $TRACEROUTE with ICMP ECHO"
	run_trace -I
}

test2()
{
	tst_res TINFO "run $TRACEROUTE with TCP SYN"
	run_trace -T
}

. tst_net.sh
tst_run
