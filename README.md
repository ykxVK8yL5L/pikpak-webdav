# pikpak-webdav
pikpak的webdav的rust实现  还有很多问题


映射端口:9867      
其它的貌似也没啥说的    
## 和阿里云的webdav一样会有缓存问题   重启dcoker即可  


示例命令:
```
docker run --name pikpak-webdav --restart=unless-stopped -p 9867:9867 -e PIKPAK_USER='XXXXXXXX' -e PIKPAK_PASSWORD='XXXXXXX' ykxvk8yl5l/pikpak-webdav:latest
```



参考项目为:
https://github.com/messense/aliyundrive-webdav
