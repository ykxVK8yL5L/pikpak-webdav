#!/bin/sh
eval `dbus export pikpakwebdav`
source /koolshare/scripts/base.sh
alias echo_date='echo $(date +%Y年%m月%d日\ %X):'
LOG_FILE=/tmp/upload/pikpakwebdavconfig.log
rm -rf $LOG_FILE
BIN=/koolshare/bin/pikpak-webdav
http_response "$1"

case $2 in
1)
    echo_date "当前已进入pikpakwebdav_config.sh" >> $LOG_FILE
    sh /koolshare/scripts/pikpakwebdavconfig.sh restart
    echo BBABBBBC >> $LOG_FILE
    ;;
esac
