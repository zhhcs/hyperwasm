需求：
1、添加一个应用注册路由：registry
    设置一个url：http://公网ip:port/registry
    公网IP暂时没有
    设计一个JSON参数，通过POST方法访问这个url，完成服务注册，返回给用户一个服务访问的url

2、给这个服务访问的url设计一个JSON参数，通过POST方法访问触发服务。

参数中需要包含：wasm模块的相关信息、预计执行时长、截止时间等
先设计JSON