#!/bin/sh
eval `dbus export pikpakwebdav_`
source /koolshare/scripts/base.sh
logger "[软件中心]: 正在卸载 pikpakwebdav..."
MODULE=pikpakwebdav
cd /
/koolshare/scripts/pikpakwebdav_config.sh stop
rm -f /koolshare/init.d/S99pikpakwebdav.sh
rm -f /koolshare/scripts/pikpakwebdav_config.sh
rm -f /koolshare/webs/Module_pikpakwebdav.asp
rm -f /koolshare/res/icon-pikpakwebdav.png
rm -f /koolshare/bin/pikpak-webdav
rm -f /koolshare/bin/pikpakwebdav.log /tmp/pikpakwebdav.log
rm -f /tmp/var/run/pikpakwebdav.pid
rm -fr /tmp/pikpakwebdav* >/dev/null 2>&1
dbus remove __event__onnatstart_pikpakwebdav
values=`dbus list pikpakwebdav | cut -d "=" -f 1`
for value in $values
do
  dbus remove $value
done
logger "[软件中心]: 完成 pikpakwebdav 卸载"
rm -f /koolshare/scripts/uninstall_pikpakwebdav.sh
