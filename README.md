# hyper-sched
仅在Ubuntu 22.04环境下运行，需要root权限
```
cargo build --release --package hyper-scheduler --example server 
sudo ./target/release/examples/server

curl -F "fib.wasm=@/home/zhanghao/dev/hyper-scheduler/examples/fib.wasm" http://127.0.0.1:3001/register

curl -F "fib.wasm=@/tmp/122.96.144.180:30080/hywasm/fib46.wasm/latest/module.wasm" http://127.0.0.1:3001/register

/call
curl -H "Content-Type: application/json" -d '{"wasm_name":"fib.wasm","task_unique_name":"fibabc","export_func":"fib_r","param_type":"i32","params":["30"],"results_length":"1","expected_execution_time":"5","expected_deadline":"35"}' -X POST http://127.0.0.1:3001/call

curl -H "Content-Type: application/json" -d '{"wasm_name":"detect.wasm","task_unique_name":"detectabc","export_func":"detect","param_type":"void","params":[],"results_length":"1","expected_execution_time":"215","expected_deadline":"300"}' -X POST http://127.0.0.1:3001/call

/test
curl -H "Content-Type: application/json" -d '{"wasm_name":"fib.wasm","export_func":"fib","param_type":"i32","params":["32"],"results_length":"1","expected_deadline":"30"}' -X POST http://127.0.0.1:3001/test

```
0. 在设置任务的预期执行时间前先测试该任务在本机环境下的执行时长

1. 建议绑定CPU运行：
   修改内核启动参数
   ```
        sudo vim /etc/default/grub
   ```
   隔离CPU0和CPU1
   ```
        GRUB_CMDLINE_LINUX="isolcpus=0,1"
   ```
   更新
   ```
        sudo update-grub
   ```
   重启后生效

3. 需要开启cgroupV2
   要授权其他控制器，如 cpu cpuset 和 io ，执行以下命令:
```
     sudo mkdir -p /etc/systemd/system/user@.service.d
     cat <<EOF | sudo tee /etc/systemd/system/user@.service.d/delegate.conf
     [Service]
     Delegate=cpu cpuset io memory pids
     EOF

     sudo systemctl daemon-reload
```

# fix
FIXME: 不支持生成子任务


1. 单独的调度线程？
2. 生成和运行的状态同步：广播、信箱


# new
1. 在register之后生成一些测试任务，测一下任务的`实际执行时长`，记录到这个wasm环境的配置中，并把这个时间返回给客户端。
     要求：1). 测试不要阻塞监听核心。
          2). 多测试几次，取得稳定的结果。
          3). 测试要在专门的测试线程，不需要封装成micro process，不要经过全局调度器，不要占用工作线程。
          4). 可能需要修改现有的结构体，但不要影响已有的功能。

     流程：1). request(register) -> benchmark -> response(register status,expected_execution_time) 
          2). request(call, relative_deadline) -> expected_execution_time < relative_deadline? -> ... -> response




          1. 冷热启动
               
                    

          2. 智能交通


          3. 吞吐量并发量     