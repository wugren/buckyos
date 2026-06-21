# key url Zone提供的一些重要的路径

## 可以匿名访问的

### Zone级别的公开内容

http://public.$zone_hostname/xxxx -> 映射到 data/srv/publish


### Zone内的DID Document



### Desktop中定义的公共URL

暂时zone公开的用户信息（用户信息公开有 完全公开，zone内可见，自己可见3个隐私登记）
https://test.buckyos.io/userprofile?user=devtest


## 发布的内容
http://$zone_hostname/pub/$named_mgr/path (GET)
http://$zone_hostname/pub/repo/meta_index.db (GET FileObj)
http://$zone_hostname/pub/repo/meta_index.db/content (GET FileObj.content)
http://$zone_hostname/pub/repo/pkg/$pkg_name/$version/chunk (GET)

## zone内的标准路径
http://$zone_hostname/ndn/$chunkid (GET | HEAD | PUT/PATCH)


## default repo (zone内)
http://$zone_hostname/ndn/repo/meta_index.db
http://$zone_hostname/ndn/repo/meta_index.db/content

## my-pub repo (zone外), 可能未签名
http://$zone_hostname/ndn/repo/pub_meta_index.db








