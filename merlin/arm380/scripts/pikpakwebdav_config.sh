#!/bin/sh
eval `dbus export pikpakwebdav`
source /koolshare/scripts/base.sh
alias echo_date='echo $(date +%Y年%m月%d日\ %X):'

BIN=/koolshare/bin/pikpak-webdav
PID_FILE=/var/run/pikpakwebdav.pid

if [ "$(cat /proc/sys/vm/overcommit_memory)"x != "0"x ];then
    echo 0 > /proc/sys/vm/overcommit_memory
fi

pikpakwebdav_start_stop(){
    if [ "${pikpakwebdav_enable}"x = "1"x ];then
        killall pikpak-webdav



        AUTH_ARGS=""
        if [ "${pikpakwebdav_auth_user}"x != ""x ];then
          AUTH_ARGS="--auth-user ${pikpak_user}"
        fi
        if [ "${pikpakwebdav_auth_password}"x != ""x ];then
          AUTH_ARGS="$AUTH_ARGS --auth-password ${pikpakwebdav_auth_password}"
        fi
        if [ "${pikpakwebdav_read_buffer_size}"x = ""x ];then
          pikpakwebdav_read_buffer_size="10485760"
        fi
        if [ "${pikpakwebdav_cache_size}"x = ""x ];then
          pikpakwebdav_cache_size="1000"
        fi
        if [ "${pikpakwebdav_root}"x = ""x ];then
          pikpakwebdav_root="/"
        fi

        start-stop-daemon -S -q -b -m -p ${PID_FILE} \
          -x /bin/sh -- -c "${BIN} --host 0.0.0.0 --port ${pikpakwebdav_port} --pikpak-user ${pikpakwebdav_user} --pikpak-password ${pikpakwebdav_password} --proxy-url ${pikpakwebdav_proxy_url} --root ${pikpakwebdav_root} --workdir /var/run/pikpakwebdav -S ${pikpakwebdav_read_buffer_size} --cache-size ${pikpakwebdav_cache_size} $AUTH_ARGS >/tmp/pikpakwebdav.log 2>&1"
    else
        killall pikpak-webdav
    fi
}

pikpakwebdav_nat_start(){
    if [ "${pikpakwebdav_enable}"x = "1"x ];then
        echo_date 添加nat-start触发事件...
        dbus set __event__onnatstart_pikpakwebdav="/koolshare/scripts/pikpakwebdav_config.sh"
    else
        echo_date 删除nat-start触发...
        dbus remove __event__onnatstart_pikpakwebdav
    fi
}

case ${ACTION} in
start)
    pikpakwebdav_start_stop
    pikpakwebdav_nat_start
    ;;
*)
    pikpakwebdav_start_stop
    pikpakwebdav_nat_start
    ;;
esac
