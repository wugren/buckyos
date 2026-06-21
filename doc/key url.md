# key url Zone提供的一些重要的路径

这里列出的是，会被外链的，需要按协议级维护的URL 

## 可以匿名访问的

### Zone级别的公开内容


http://public.$zone_hostname/xxxx -> 映射到 data/srv/publish




### Zone内的公开实体的DID Document (json链接)

- 域名是合法did时，获取对应的did

- 查询任意did(原则上只包含zone内实体)



### Desktop中定义的公共URL

- 用户邀请链接
- share_content相关页面 （实体内容是如何引用的？）


### zone内的 ndn 标准路径
http://$zone_hostname/ndn/$chunkid (GET | HEAD | PUT/PATCH)


### 用户（实体）profile

https://$zonehost/profile?id=xxxx
https://test.buckyos.io/userprofile?user=devtest （现在情况）

### 给Zone投递NamedObject (sendmsg)

## 用户首页（和用户的默认app有关）

https://$username.$zonehost/

用户的username是无法改变的，能修改的是nickname/show name/fullname这类

## app url

root用户安装的app

https://$appid.$zonehost/ 


为特定用户安装的app

https://$appid-$userid.$zonehost/







