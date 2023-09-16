# hyper-sched
仅在Ubuntu 22.04环境下运行，需要root权限
```
cargo build --release --package hyper-scheduler --example server 
sudo ./target/release/examples/server

curl -F "fib.wasm=@/home/ubuntu/dev/hyper-scheduler/examples/fib.wasm" http://127.0.0.1:3001/register

curl -H "Content-Type: application/json" -d '{"wasm_name":"fib.wasm","task_unique_name":"fibabc","export_func":"fib","param_type":"i32","params":["30"],"results_length":"1","expected_execution_time":"20","relative_deadline":"30"}' -X POST http://127.0.0.1:3001/call

这一条过时了
curl -H "Content-Type: application/json" -d '{"wasm_name":"fib.wasm","export_func":"fib","param_type":"i32","params":["32"],"results_length":"1"}' -X POST http://127.0.0.1:3001/call

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

