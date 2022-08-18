
# pikpak-webdav
演示视频:https://youtu.be/Fkms3_qanZQ   
pikpak的webdav的rust实现  还有很多问题  ~~不支持复制、上传等功能主要是视频播放~~   
新版本已经支持文件的上传、删除等功能，但是由于新加依赖，不确定是否可以支持更多的设备。    
代码质量极其低下，请尽量不要使用       
https://hub.docker.com/r/ykxvk8yl5l/pikpak-webdav
###  群晖Docker可能会运行失败 请直接在命令行执行docker run命令 或去除Docker管理界面的path变量演示视频:https://youtu.be/M2OFB1gYU4w
## 有网友反馈非邮箱登陆会登陆失败，请使用邮箱、密码方式登陆  google账号登陆的记得到pikpak后台设置密码
映射端口:9867      
其它的貌似也没啥说的  

## 命令行格式[然后浏览器访问http://localhost:9867】 可选参数 --proxy-url xxxxxxxx     
```
pikpak-webdav --host 0.0.0.0 --pikpak-user xxxx --pikpak-password xxxx 
```

## 可用代理【未验证】 
https://api.13pikpak.cf   
https://api.14pikpak.cf  
https://api.15pikpak.cf   
https://api.16pikpak.cf   
https://api.17pikpak.cf   
https://api.18pikpak.cf       
https://cors.z7.workers.dev      
https://cors.z13.workers.dev   
https://cors.z14.workers.dev   
https://cors.z15.workers.dev   
https://cors.z16.workers.dev   
https://cors.z17.workers.dev   
https://cors.z18.workers.dev   
https://pikpak.2509652494538.workers.dev


## 安装

可以从 [GitHub Releases](https://github.com/ykxVK8yL5L/pikpak-webdav/releases) 页面下载预先构建的二进制包， 也可以使用 pip 从 PyPI 下载(python几乎不更新功能不是最新):

```bash
pip install pikpak-webdav
```



Docker示例命令【如需代理加入-e PROXY_URL='XXXXXXXXX'】:
```
docker run --name pikpak-webdav --restart=unless-stopped -p 9867:9867 -e PIKPAK_USER='XXXXXXXX' -e PIKPAK_PASSWORD='XXXXXXX' ykxvk8yl5l/pikpak-webdav:latest
```


openwrt   
x86_64的安装代码:   
```
wget https://github.com/ykxVK8yL5L/pikpak-webdav/releases/download/v0.0.2/pikpak-webdav_0.0.2_x86_64.ipk
wget https://github.com/ykxVK8yL5L/pikpak-webdav/releases/download/v0.0.2/luci-app-pikpak-webdav_1.0.0_all.ipk
wget https://github.com/ykxVK8yL5L/pikpak-webdav/releases/download/v0.0.2/luci-i18n-pikpak-webdav-zh-cn_1.0.0-1_all.ipk
opkg install pikpak-webdav_0.0.2_x86_64.ipk
opkg install luci-app-pikpak-webdav_1.0.0_all.ipk
opkg install luci-i18n-pikpak-webdav-zh-cn_1.0.0-1_all.ipk
```

其它 CPU 架构的路由器可在 [GitHub Releases](https://github.com/ykxVK8yL5L/pikpak-webdav/releases) 页面中查找对应的架构的主程序 ipk 文件下载安装， 常见
OpenWrt 路由器 CPU 架构如下表（欢迎补充）：

|      路由器     |        CPU 架构       |
|----------------|----------------------|
| nanopi r4s     | aarch64_generic      |
| 小米 AX3600     | aarch64_cortex-a53  |
| 斐讯 N1 盒子    | aarch64_cortex-a53   |
| Newifi D2      | mipsel_24kc          |
| Pogoplug       | arm_mpcore           |

> Tips: 不清楚 CPU 架构类型可通过运行 `opkg print-architecture` 命令查询。


## Koolshare 梅林固件

[GitHub Releases](https://github.com/ykxVK8yL5L/pikpak-webdav/releases/tag/v0.0.2) 中有预编译包 `pikpakwebdav-merlin-arm*.tar.gz`
， 目前提供了旧的 arm380 和兼容 arm384/386 固件的版本，可在下载后在软件中心离线安装。


参考项目为:
https://github.com/messense/aliyundrive-webdav
