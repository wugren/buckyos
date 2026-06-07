## 定义标准协议

client:
read(obj_uri) -> json
read(obj_profile_uri) -> json


server:

URL规范
1）Obj是一个按objid独立的“资源” ： `http://myhome.com/devices/cam01`,通过  `http://myhome.com/devices/cam01/did.json` 可以得到meta信息
{
  "@context": [
    "https://www.w3.org/ns/did/v1",
    "https://buckyos.org/ns/global-object/v1"
  ],
  "id": "did:web:myhome.com:devices:cam01",
  "alsoKnownAs": ["http://myhome.com/devices/cam01"],
  "controller": "did:web:myhome.com",
  "verificationMethod": [],
  "service": [
    {
      "id": "#global-object",
      "type": "GlobalObjectService",
      "serviceEndpoint": "http://myhome.com/devices/cam01",
      //"object-meta":"obj.json" 无profile时使用(每个object有独立的profile)
      "profile": "https://buckyos.org/profiles/web-camera@1",
      "kind": "web.camera"
    }
  ]
}




2）同类型的Object共享Obj-Profile,Profile中可以约定起属性/方法/事件的访问方法 ,比如profile是 web-camrer,定义有 
{
    "methods":{
        "query_clip": {
            "endpoint" : "query_clip"
            ...
        }
    },
    "events":{
        "on_low_battery":{
            "endpoint":"events"
            ...
        }...

    },
    "props":{
        "brand":{
            "desc":"",
        }
    }
}

(endpoint不写，也可以基于Obj URL自动拼写出来)
(基本兼容 W3C WoT Thing Description)

3）识别后，用下面具体协议访问

- 方法 POST http://myhome.com/devices/cam01/query_clip 
- 事件 ws://myhome.com/devices/cam01/events
- 属性 GET http://myhome.com/devices/cam01/brand

## 在buckyos-base中进行实现

1）协议实现
- did-obj-card
- obj-profile

2) 标准client & server

定义标准的server

client似乎不用定义标准的，可以在server的测试用例里进行验证



### 实现 krpc-call





## 定义local adapter

## 实现global_object_runtime

### 实现read

### 实现