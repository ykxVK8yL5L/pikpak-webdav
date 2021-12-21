# pikpak-webdav
演示视频:https://youtu.be/Fkms3_qanZQ   
pikpak的webdav的rust实现  还有很多问题 不支持复制、上传等功能主要是视频播放        
https://hub.docker.com/r/ykxvk8yl5l/pikpak-webdav

映射端口:9867      
其它的貌似也没啥说的  

## 命令行格式[然后浏览器访问http://localhost:9867】      
```
pikpak-webdav --host 0.0.0.0 --pikpak-user xxxx --pikpak-password xxxx 
```

## 安装

可以从 [GitHub Releases](https://github.com/ykxVK8yL5L/pikpak-webdav/releases) 页面下载预先构建的二进制包， 也可以使用 pip 从 PyPI 下载:

```bash
pip install pikpak-webdav
```



示例命令:
```
docker run --name pikpak-webdav --restart=unless-stopped -p 9867:9867 -e PIKPAK_USER='XXXXXXXX' -e PIKPAK_PASSWORD='XXXXXXX' ykxvk8yl5l/pikpak-webdav:latest
```


openwrt 目前支持:x86_64和aarch64_generic其它的有需要的话  提issue吧     
x86_64的安装代码:   
```
wget https://github.com/ykxVK8yL5L/pikpak-webdav/releases/download/v0.0.1/pikpak-webdav_0.0.1_x86_64.ipk
wget https://github.com/ykxVK8yL5L/pikpak-webdav/releases/download/v0.0.1/luci-app-pikpak-webdav_1.0.0_all.ipk
wget https://github.com/ykxVK8yL5L/pikpak-webdav/releases/download/v0.0.1/luci-i18n-pikpak-webdav-zh-cn_1.0.0-1_all.ipk
opkg install pikpak-webdav_0.0.1_x86_64.ipk
opkg install luci-app-pikpak-webdav_1.0.0_all.ipk
opkg install luci-i18n-pikpak-webdav-zh-cn_1.0.0-1_all.ipk
```


参考项目为:
https://github.com/messense/aliyundrive-webdav
