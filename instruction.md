1. 环境优化(可选)：
    (1) 隔离CPU并绑定：
        
        ```bash
           修改内核启动参数
           
                sudo vim /etc/default/grub
        
           隔离CPU0和CPU1
           
                GRUB_CMDLINE_LINUX="isolcpus=0,1"
          
           更新
           
                sudo update-grub
           
           重启后生效
        ```
        
    (2) 开启cgroupV2并授权其他控制器cpu、cpuset
        
        ```bash
        sudo mkdir -p /etc/systemd/system/user@.service.d
             cat <<EOF | sudo tee /etc/systemd/system/user@.service.d/delegate.conf
             [Service]
             Delegate=cpu cpuset io memory pids
             EOF
        
             sudo systemctl daemon-reload
        ```
        

2. 配置kubelet，指定criSocket
    
    ```bash
    修改 /etc/systemd/system/kubelet.service
    
    [Unit]
    Description=Kubernetes Kubelet
    Documentation=https://github.com/kubernetes/kubernetes
    
    [Service]
    ExecStart=/usr/local/bin/kubelet \
      --config=/etc/kubernetes/kubelet-config.yaml \
      --container-runtime=remote \
      --container-runtime-endpoint=unix:///var/run/hyperwasm.sock \
      --image-pull-progress-deadline=2m \
      --kubeconfig=/etc/kubernetes/kubeconfig \
      --network-plugin=cni \
      --node-ip=${IP} \
      --register-node=true \
      --v=2
    Restart=on-failure
    RestartSec=5
    
    [Install]
    WantedBy=multi-user.target
    
    启动服务
    systemctl daemon-reload
    systemctl restart kubelet
    ```
    
3. 启动 hypercool
    
    ```bash
    sudo ./hypercool
    ```
    
4. 启动 server
    
    ```bash
    sudo ./server
    ```
    
5. 运行
    
    ```bash
    sudo kubectl apply -f example.yaml
    ```
    
    ```yaml
    apiVersion: v1
    kind: Pod
    metadata:
      name: fib33
      labels:
        app: fib33
    spec:
      containers:
        - image: fib33:v1.0.0
          imagePullPolicy: IfNotPresent
          name: fib33
          env:
          - name: expected-execution  #预期执行时间ms
            value: 12
          - name: relative-ddl  #相对截止时间ms
            value: 50
          - name: func  #导出函数(可选)
            value: fib33
      tolerations:
        - key: "node.kubernetes.io/network-unavailable"
          operator: "Exists"
          effect: "NoSchedule"
        - key: "kubernetes.io/arch"
          operator: "Equal"
          value: "wasm32-wasi"
          effect: "NoExecute"
        - key: "kubernetes.io/arch"
          operator: "Equal"
          value: "wasm32-wasi"
          effect: "NoSchedule"
    ```
    
6. url输出到日志中
    
    ```bash
    INFO hyper_scheduler::axum::client: Response: 0.0.0.0:3001/fib33
    ```
    
    ```bash
    $ curl http://0.0.0.0:3001/fib33
    
    new task spawned id: 1, name: fib33-WQKi6Z7
    
    $ curl http://0.0.0.0:3000/uname?uname=fib33-WQKi6Z7
    
    some status
    ```