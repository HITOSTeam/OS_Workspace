#!/bin/bash
################################################################################
##                                                                            ##
## Copyright (c) Dan Carpenter., 2004                                         ##
##                                                                            ##
## This program is free software;  you can redistribute it and#or modify      ##
## it under the terms of the GNU General Public License as published by       ##
## the Free Software Foundation; either version 2 of the License, or          ##
## (at your option) any later version.                                        ##
##                                                                            ##
## This program is distributed in the hope that it will be useful, but        ##
## WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY ##
## or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License   ##
## for more details.                                                          ##
##                                                                            ##
## You should have received a copy of the GNU General Public License          ##
## along with this program;  if not, write to the Free Software               ##
## Foundation, Inc., 51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA    ##
##                                                                            ##
################################################################################

## DESCRIPTION:
## This test creates 20 files (0 thru 19) and then shuffles them around,
## deletes, and recreates them as fast as possible.  This is all done in
## an effort to test for race conditions in the filesystem code. This test
## runs until killed or Ctrl-C'd.  It is suggested that it run overnight
## with preempt turned on to make the system more sensitive to race
## conditions.

MAX_FILES=20
CLEAR_SECS=30
DIR="$TMPDIR/race"
FS_RACER_GROUP=all

run_group()
{
case "$1" in
    create)
        ./fs_racer_file_create.sh $DIR $MAX_FILES &
        ./fs_racer_file_create.sh $DIR $MAX_FILES &
        ./fs_racer_file_create.sh $DIR $MAX_FILES &
        ;;
    dir)
        ./fs_racer_dir_create.sh $DIR $MAX_FILES &
        ./fs_racer_dir_create.sh $DIR $MAX_FILES &
        ./fs_racer_dir_create.sh $DIR $MAX_FILES &
        ;;
    rename)
        ./fs_racer_file_rename.sh $DIR $MAX_FILES &
        ./fs_racer_file_rename.sh $DIR $MAX_FILES &
        ./fs_racer_file_rename.sh $DIR $MAX_FILES &
        ;;
    link)
        ./fs_racer_file_link.sh $DIR $MAX_FILES &
        ./fs_racer_file_link.sh $DIR $MAX_FILES &
        ./fs_racer_file_link.sh $DIR $MAX_FILES &
        ;;
    symlink)
        ./fs_racer_file_symlink.sh $DIR $MAX_FILES &
        ./fs_racer_file_symlink.sh $DIR $MAX_FILES &
        ./fs_racer_file_symlink.sh $DIR $MAX_FILES &
        ;;
    concat)
        ./fs_racer_file_concat.sh $DIR $MAX_FILES &
        ./fs_racer_file_concat.sh $DIR $MAX_FILES &
        ./fs_racer_file_concat.sh $DIR $MAX_FILES &
        ;;
    list)
        ./fs_racer_file_list.sh $DIR &
        ./fs_racer_file_list.sh $DIR &
        ./fs_racer_file_list.sh $DIR &
        ;;
    rm)
        ./fs_racer_file_rm.sh $DIR $MAX_FILES &
        ./fs_racer_file_rm.sh $DIR $MAX_FILES &
        ./fs_racer_file_rm.sh $DIR $MAX_FILES &
        ;;
    all)
        run_group create
        run_group dir
        run_group rename
        run_group link
        run_group symlink
        run_group concat
        run_group list
        run_group rm
        ;;
    *)
        echo "unknown fs_racer group: $1"
        exit 1
        ;;
esac
}

run_groups()
{
groups="$1"
while [ -n "$groups" ]; do
    group=${groups%%,*}
    if [ "$group" = "$groups" ]; then
        groups=
    else
        groups=${groups#*,}
    fi
    run_group "$group"
done
}

execute_test()
{
[ -e $DIR ] || mkdir $DIR
run_groups "$FS_RACER_GROUP"
}


usage()
{
    echo usage: fs_racer.sh [-g GROUP] -t DURATION [Execute the testsuite for given DURATION seconds]
    exit 0;
}


call_exit()
{
    echo \"Cleaning up\"
    killall fs_racer_file_create.sh
    killall fs_racer_dir_create.sh
    killall fs_racer_file_rm.sh
    killall fs_racer_file_rename.sh
    killall fs_racer_file_link.sh
    killall fs_racer_file_symlink.sh
    killall fs_racer_file_list.sh
    killall fs_racer_file_concat.sh
    rm -rf $DIR
    exit 0
}

DURATION=
while getopts :t:g: arg
do  case $arg in
    t)  DURATION=$OPTARG;;
    g)  FS_RACER_GROUP=$OPTARG;;
    \?) usage;;
    esac
done

if [ -n "$DURATION" ]; then
    execute_test
    sleep $DURATION
    call_exit
fi

exit 0
