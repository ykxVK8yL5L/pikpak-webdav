#!/bin/sh
source /koolshare/scripts/base.sh
MODULE=pikpakwebdav
DIR=$(cd $(dirname $0); pwd)

cd /tmp
killall pikpak-webdav
rm -f /koolshare/bin/pikpakwebdav.log
cp -rf /tmp/pikpakwebdav/bin/* /koolshare/bin/
cp -rf /tmp/pikpakwebdav/scripts/* /koolshare/scripts/
cp -rf /tmp/pikpakwebdav/webs/* /koolshare/webs/
cp -rf /tmp/pikpakwebdav/res/* /koolshare/res/

chmod a+x /koolshare/bin/pikpak-webdav
chmod a+x /koolshare/scripts/pikpakwebdav_config.sh
chmod a+x /koolshare/scripts/uninstall_pikpakwebdav.sh
ln -sf /koolshare/scripts/pikpakwebdav_config.sh /koolshare/init.d/S99pikpakwebdav.sh

dbus set softcenter_module_${MODULE}_name="${MODULE}"
dbus set softcenter_module_${MODULE}_title="PIKPAK网盘WebDAV"
dbus set softcenter_module_${MODULE}_description="PIKPAK网盘 WebDAV 服务器"
dbus set softcenter_module_${MODULE}_version="$(cat $DIR/version)"
dbus set softcenter_module_${MODULE}_install="1"

# 默认配置
dbus set ${MODULE}_port="8080"
dbus set ${MODULE}_read_buffer_size="10485760"
dbus set ${MODULE}_cache_size="1000"

rm -rf /tmp/pikpakwebdav* >/dev/null 2>&1
aw_enable=`dbus get pikpakwebdav_enable`
if [ "${aw_enable}"x = "1"x ];then
    /koolshare/scripts/pikpakwebdav_config.sh
fi
logger "[软件中心]: 完成 pikpakwebdav 安装"
exit
