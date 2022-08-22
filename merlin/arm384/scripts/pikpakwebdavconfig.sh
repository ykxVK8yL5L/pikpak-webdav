#!/bin/sh
eval `dbus export pikpakwebdav`
source /koolshare/scripts/base.sh
alias echo_date='echo $(date +%Y年%m月%d日\ %X):'
LOG_FILE=/tmp/upload/pikpakwebdavconfig.log
BIN=/koolshare/bin/pikpak-webdav

if [ "$(cat /proc/sys/vm/overcommit_memory)"x != "0"x ];then
    echo 0 > /proc/sys/vm/overcommit_memory
fi

pikpakwebdav_start_stop(){
    echo_date "当前已进入pikpakwebdavconfig.sh执行" >> $LOG_FILE
    if [ "${pikpakwebdav_enable}"x = "1"x ];then
        echo_date "先结束进程" >> $LOG_FILE
        killall pikpak-webdav


        AUTH_ARGS=""
        if [ "${pikpakwebdav_auth_user}"x != ""x ];then
          AUTH_ARGS="--auth-user ${pikpakwebdav_auth_user}"
        fi
        if [ "${pikpakwebdav_auth_password}"x != ""x ];then
          AUTH_ARGS="$AUTH_ARGS --auth-password ${pikpakwebdav_auth_password}"
        fi
        if [ "${pikpakwebdav_read_bufffer_size}"x = ""x ];then
          pikpakwebdav_read_bufffer_size="10485760"
        fi
        if [ "${pikpakwebdav_write_bufffer_size}"x = ""x ];then
          pikpakwebdav_write_bufffer_size="16777216"
        fi

        if [ "${pikpakwebdav_cache_size}"x = ""x ];then
          pikpakwebdav_cache_size="1000"
        fi
        if [ "${pikpakwebdav_cache_ttl}"x = ""x ];then
          pikpakwebdav_cache_ttl="600"
        fi
        if [ "${pikpakwebdav_root}"x = ""x ];then
          pikpakwebdav_root="/"
        fi


        echo_date "参数为：${pikpakwebdav_port} --pikpak-user ${pikpakwebdav_user} --pikpak-password ${pikpakwebdav_password} --proxy-url ${pikpakwebdav_proxy_url} --root ${pikpakwebdav_root} -S ${pikpakwebdav_read_buffer_size} --upload-buffer-size ${pikpakwebdav_write_buffer_size} --cache-size ${pikpakwebdav_cache_size} --cache-ttl ${pikpakwebdav_cache_ttl} $AUTH_ARGS" >> $LOG_FILE
        #start-stop-daemon -S -q -b -m -p ${PID_FILE} \
        #  -x /bin/sh -- -c "${BIN} --workdir /var/run/pikpakwebdav --host 0.0.0.0 --p ${pikpakwebdav_port} --pikpak-user ${pikpakwebdav_user} --pikpak-password ${pikpakwebdav_password} --proxy-url ${pikpakwebdav_proxy_url} --cache-ttl ${pikpakwebdav_cache_ttl} --root ${pikpakwebdav_root} -S ${pikpakwebdav_read_bufffer_size} --upload-buffer-size ${pikpakwebdav_write_buffer_size}  $AUTH_ARGS >/tmp/pikpakwebdav.log 2>&1"
        ${BIN}  --workdir /var/run/pikpakwebdav --host 0.0.0.0 --port ${pikpakwebdav_port} --pikpak-user ${pikpakwebdav_user} --pikpak-password ${pikpakwebdav_password} --proxy-url ${pikpakwebdav_proxy_url} --cache-ttl ${pikpakwebdav_cache_ttl} --root ${pikpakwebdav_root} -S ${pikpakwebdav_read_buffer_size} --upload-buffer-size ${pikpakwebdav_write_buffer_size}  --cache-size ${pikpakwebdav_cache_size} $AUTH_ARGS >/tmp/upload/pikpakwebdav.log 2>&1 &
        sleep 5s
        if [ ! -z "$(pidof pikpak-webdav)" -a ! -n "$(grep "Error" /tmp/upload/pikpakwebdav.log)" ] ; then
          echo_date "PikPak 进程启动成功！(PID: $(pidof pikpak-webdav))" >> $LOG_FILE
          if [ "$pikpakwebdav_public" == "1" ]; then
            iptables -I INPUT -p tcp --dport $pikpakwebdav_port -j ACCEPT >/dev/null 2>&1 &
          else
            iptables -D INPUT -p tcp --dport $pikpakwebdav_port -j ACCEPT >/dev/null 2>&1 &
          fi
        else
          echo_date "PikPak 进程启动失败！请检查参数是否存在问题，即将关闭" >> $LOG_FILE
          echo_date "失败原因：" >> $LOG_FILE
          error1=$(cat /tmp/upload/pikpakwebdav.log | grep -ioE "Error.*")
          if [ -n "$error1" ]; then
              echo_date $error1 >> $LOG_FILE
          fi
          dbus set pikpakwebdav_enable="0"
        fi
    else
        killall pikpak-webdav
        iptables -D INPUT -p tcp --dport $pikpakwebdav_port -j ACCEPT >/dev/null 2>&1 &
    fi
}
pikpakwebdav_stop(){
  killall pikpak-webdav
  iptables -D INPUT -p tcp --dport $pikpakwebdav_port -j ACCEPT >/dev/null 2>&1 &
}


case $ACTION in
start)
    pikpakwebdav_start_stop
    echo BBABBBBC >> $LOG_FILE
    ;;
start_nat)
    pikpakwebdav_start_stop
    echo BBABBBBC >> $LOG_FILE
    ;;
restart)
    pikpakwebdav_start_stop
    ;;
stop)
    pikpakwebdav_stop
    echo BBABBBBC >> $LOG_FILE
    ;;
*)
    pikpakwebdav_start_stop
    echo BBABBBBC >> $LOG_FILE
    ;;
esac
