#!/bin/sh

export KSROOT=/koolshare
source $KSROOT/scripts/base.sh
eval $(dbus export pikpakwebdav_)
alias echo_date='echo 【$(date +%Y年%m月%d日\ %X)】:'

pid_ali=$(pidof pikpak-webdav)
date=$(echo_date)

if [ -n "$pid_ali" ]; then
    text1="<span style='color: #6C0'>$date PIKPAK网盘 进程运行正常！(PID: $pid_ali)</span>"
else
    text1="<span style='color: red'>$date PIKPAK网盘 进程未在运行！</span>"
fi

aliversion=$(/koolshare/bin/pikpak-webdav -V 2>/dev/null | head -n 1 | cut -d " " -f2)
if [ -n "$aliversion" ]; then
	aliversion="$aliversion"
else
	aliversion="null"
fi
dbus set pikpakwebdav_version="$aliversion"

http_response "$text1@$aliversion@"
