# hyper-sched
仅在Ubuntu 22.04环境下运行，需要root权限
```
cargo build --release --package hyper-scheduler --example server 
sudo ./target/release/examples/server
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
# 进度安排

第一周（7/3-7/7）：

1. 添加任务队列：设计并实现一个基本的任务队列，用于存储待执行的任务。
2. 添加调度信号：定义一个调度信号，用于通知调度器有新任务可执行。
3. 实现简单调度器：创建一个简单的调度器，负责从任务队列中选择任务并执行。

第二周（7/10-7/14）：

1. 提供入口：设计一个入口函数，能够接收用户定义的函数，并将其加入任务队列等待调度执行。
2. 完善任务队列：根据任务的不同状态（例如等待、就绪、运行、完成），完善任务队列的逻辑，确保任务按照正确的状态进行调度。
3. 处理时钟中断和其他信号：研究和实现对时钟中断等外部信号的处理机制，以便调度器能够适应不同的事件和情况。

第三周（7/17-7/21）：

1. 设计任务的运行规则和时间：定义任务的运行规则，例如任务的优先级、最大执行时间等。
2. 实现最早截止优先调度：根据任务的截止时间，设计并实现一个最早截止优先调度算法，确保任务按照截止时间的先后顺序得到调度和执行。

第四周（7/24-7/28）：
1. 统计任务的各种状态：跟踪和记录任务的各种状态，如运行时间、等待时间、完成时间等。
2. 实现资源监视器：设计和实现一个资源监视器，显示统计信息。
3. 对接

第五周（7/31-8/4）：
1. 内存管理：研究和实现合适的内存管理机制，包括分配和释放内存等功能。
2. 错误处理。
0. (暂时不做)处理任务共享资源：设计并实现对任务共享资源的管理和调度，确保任务能够正确地访问和利用共享资源。

TODO: name和id对应

# fix
FIXME1: 在其他线程生成的coroutine，如果已经生成上下文，那么当前worker获取这个coroutine并运行就会panic
        解决办法：生成coroutine但不生成上下文，先加入pending队列，等运行时再生成上下文。
        还有别的办法吗

FIXME2: 不支持生成子任务


1. 单独的调度线程？
2. 生成和运行的状态同步：广播、信箱

curl -F "fib.wasm=@/home/ubuntu/dev/hyper-scheduler/examples/fib.wasm" http://127.0.0.1:3001/register

curl -H "Content-Type: application/json" -d '{"wasm_name":"fib.wasm","export_func":"fib","param_type":"i32","params":["30"],"results_length":"1"}' -X POST http://127.0.0.1:3001/call

curl -H "Content-Type: application/json" -d '{"wasm_name":"fib.wasm","task_unique_name":"fibabc","export_func":"fib","param_type":"i32","params":["30"],"expected_execution_time":"20","relative_deadline":"30"}' -X POST http://127.0.0.1:3001/fib.wasm