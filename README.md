# hyper-sched
单工作线程、确定性计算
假设每个任务都有确定的执行时长和实时要求；工作线程绑定CPU，不会被其他线程抢占
1、用Task包装Coroutine，添加任务号、运行状态、运行时长、开始和截止时间等，这些状态需要对外暴露；
2、最早截止优先调度：主线程（监控线程）处理突发任务：将任务分发到工作线程，按需抢占；
3、时间片轮转调度：工作线程是否可以自己设置定时器，定时切换任务；