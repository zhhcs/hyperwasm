use crate::{
    scheduler::Scheduler,
    task::{Coroutine, SchedulerStatus},
    StackSize,
};
use anyhow::Error;
use once_cell::sync::Lazy;
use std::{
    collections::{BTreeMap, BinaryHeap, HashMap},
    panic::{self, AssertUnwindSafe},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

static _AVA_TIME: Lazy<Mutex<HashMap<u64, f64>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Runtime就是Runtime
pub struct Runtime {
    scheduler: Arc<Scheduler>,
    threads: Vec<JoinHandle<()>>,
}

impl Default for Runtime {
    /**
     * 默认单线程,无定时器
     */
    fn default() -> Self {
        let scheduler = Scheduler::new(1, 0);
        let threads = Scheduler::start(&scheduler, 0);
        Self { scheduler, threads }
    }
}

impl Runtime {
    /**
     * 需要指定线程数量和定时器周期（微秒）
     */
    pub fn new(
        worker_threads: Option<u8>,
        start_cpu: Option<u8>,
        timer_exp: Option<u64>,
    ) -> Runtime {
        let scheduler = Scheduler::new(
            worker_threads.unwrap_or_default(),
            start_cpu.unwrap_or_default(),
        );
        let threads = Scheduler::start(&scheduler, timer_exp.unwrap_or_default());
        Runtime { scheduler, threads }
    }

    /**
     * 准入控制的结果
     */
    pub fn admission_control_result(
        &self,
        expected_execution_time: Option<Duration>,
        relative_deadline: Option<Duration>,
    ) -> SchedulabilityResult {
        // 如果不是实时任务那就随便调度吧
        if relative_deadline.is_none() || expected_execution_time.is_none() {
            return SchedulabilityResult {
                ac: AdmissionControl::NOTREALTIME,
                worker_id: None,
                costatus: None,
            };
        }
        // 新建这个任务的状态并初始化id
        let mut co_stat = SchedulerStatus::new(expected_execution_time, relative_deadline);
        let id = crate::task::get_id();
        co_stat.init(id);
        // 准入控制
        self.is_schedulable(&co_stat)
    }

    /**
     * microprocess的实例化
     */
    pub fn micro_process<F, T>(
        &self,
        f: F,
        schedulability_result: SchedulabilityResult,
    ) -> Result<u64, Error>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
    {
        // 捕获panic
        let func = Box::new(move || {
            let _ = panic::catch_unwind(AssertUnwindSafe(f));
        });
        // 获取准入控制结果
        let ac = schedulability_result.get_ac();
        // 获取调度的目标工作核心
        let worker_id = schedulability_result.worker_id.unwrap_or_default();
        // 这个状态用于实例化
        let status = schedulability_result.costatus;
        match ac {
            AdmissionControl::NOTREALTIME => {
                tracing::info!("NOT REAL TIME");
                let co = Coroutine::new(func, StackSize::default(), false, None, None);
                let id = co.get_co_id();
                // 这里的worker_id没用
                if let Ok(()) = self.scheduler.push(co, false, worker_id) {
                    // 直接丢到全局非实时microprocess队列
                    return Ok(id);
                } else {
                    tracing::error!("spawn failed");
                    return Err(Error::msg("spawn failed"));
                };
            }
            AdmissionControl::PREEMPTIVE => {
                let co = Coroutine::from_status(func, status.unwrap());
                let id = co.get_co_id();
                let stat = co.get_schedulestatus();
                // 先更新状态
                self.scheduler.update_status(id, stat, worker_id);
                // 放到对应工作核心的slot
                self.scheduler.set_slots(worker_id, co);

                // 发信号通知抢占
                let sigval = libc::sigval {
                    sival_ptr: 0 as *mut libc::c_void,
                };
                if let Some(pthread_id) = self.scheduler.get_pthread_id(worker_id) {
                    let ret = unsafe {
                        libc::pthread_sigqueue(
                            pthread_id,
                            crate::scheduler::PREEMPTY as i32,
                            sigval,
                        )
                    };
                    assert!(ret == 0);
                    return Ok(id);
                }
                Err(Error::msg("spawn failed"))
            }

            AdmissionControl::SCHEDULABLE => {
                let co = Coroutine::from_status(func, status.unwrap());
                let stat = co.get_schedulestatus();
                let id = co.get_co_id();
                // 放到目标工作核心的实时队列排队
                if let Ok(()) = self.scheduler.push(co, true, worker_id) {
                    self.scheduler.update_status(id, stat, worker_id);
                    return Ok(id);
                } else {
                    tracing::error!("spawn failed");
                    return Err(Error::msg("spawn failed"));
                };
            }
            _ => {
                // 这个case永远不会到达
                return Err(Error::msg("spawn failed, cause: UNSCHEDULABLE"));
            }
        }
    }

    /**
     * 准入控制
     */
    fn is_schedulable(&self, co_stat: &SchedulerStatus) -> SchedulabilityResult {
        //TODO: 指定一个worker，怎么选？或者可以遍历所有的线程
        // 这里直接随机指定一个
        let worker_id = (co_stat.get_co_id() % self.threads.len() as u64) as u8;
        while let Some(mut status_map) = self.scheduler.get_status(worker_id) {
            //获取调度器的任务状态信息并进入循环，没有任务状态信息，循环将退出。
            if status_map.is_empty() {
                //如果任务状态信息为空，表示当前没有其他任务在运行，因此可以直接调度新任务。
                //计算并更新可用时间
                let available_time = (co_stat.absolute_deadline.unwrap()
                    - Instant::now()
                    - co_stat.expected_remaining_execution_time.unwrap())
                .as_micros() as i128 as f64;
                self.scheduler
                    .update_ava_time(worker_id, co_stat.get_co_id(), available_time);

                return SchedulabilityResult {
                    ac: AdmissionControl::SCHEDULABLE,
                    worker_id: Some(worker_id),
                    costatus: Some(co_stat.clone()),
                };
            }
            let curr: u64 = self.scheduler.get_curr_running_id(worker_id); //获取当前正在运行的任务的唯一标识符

            let running = status_map.get(&curr); //获取当前运行的任务的状态信息
            if running.is_none() {
                //如果当前没有正在运行的任务，或者没有启动时间信息，则跳过循环并继续。
                //没有获取到可能是任务刚开始
                drop(status_map);
                continue;
            }
            let start = running.unwrap().curr_start_time;
            if start.is_none() {
                //同上,可能是任务刚开始
                drop(status_map);
                continue;
            }
            let start = start.unwrap(); //当前运行任务启动时间
            let now = Instant::now(); //当前时间
            if status_map.get(&curr).unwrap().absolute_deadline.is_some() {
                //如果当前运行的任务有绝对截止日期，说明它是实时任务
                status_map.entry(curr).and_modify(|curr_stat| {
                    let mut eret = curr_stat.expected_remaining_execution_time.unwrap(); //获取剩余执行时间
                    let time_diff = now - start;
                    if eret > time_diff {
                        //剩余执行时间 eret 大于时间差
                        eret -= time_diff;
                        curr_stat.expected_remaining_execution_time = Some(eret);
                    //更新剩余执行时间
                    } else {
                        //如果剩余执行时间小于等于时间差，将剩余执行时间设置为零。
                        //说明任务已经结束
                        curr_stat.expected_remaining_execution_time =
                            Some(std::time::Duration::from_millis(0));
                    }
                });
            } else {
                //如果当前运行任务没有绝对截止日期，可以被抢占
                // 抢占前更新可用时间
                let available_time = (co_stat.absolute_deadline.unwrap()
                    - now
                    - co_stat.expected_remaining_execution_time.unwrap())
                .as_micros() as i128 as f64;
                self.scheduler
                    .update_ava_time(worker_id, co_stat.get_co_id(), available_time);

                return SchedulabilityResult {
                    ac: AdmissionControl::PREEMPTIVE,
                    worker_id: Some(worker_id),
                    costatus: Some(co_stat.clone()),
                };
            }

            // 以下开始是实时任务的准入控制
            let mut stat_vec = BinaryHeap::new(); //创建一个二叉堆存储任务的状态信息
            status_map.iter().for_each(|(_, s)| {
                //迭代任务状态信息，将具有绝对截止日期的任务状态信息放入堆中。
                if s.absolute_deadline.is_some() {
                    stat_vec.push(s)
                }
            });
            let mut total_remaining: f64 = 0.0; //任务的总剩余执行时间

            //快速判断：如果𝑑_𝑛𝑒𝑤 - 𝑑_𝑙𝑎𝑠𝑡 ≥ 𝐶_𝑛𝑒𝑤，直接准入
            if let Some(end_ddl) = self.scheduler.get_end_ddl(worker_id) {
                // TODO:get_end_ddl获取了最新的状态，是否可以用之前的status_map代替？

                if co_stat.expected_remaining_execution_time.unwrap()
                    <= co_stat.absolute_deadline.unwrap() - end_ddl
                {
                    while let Some(s) = stat_vec.pop() {
                        total_remaining +=
                            s.expected_remaining_execution_time.unwrap().as_micros() as i128 as f64;
                    }
                    let available_time = (co_stat.absolute_deadline.unwrap() - now).as_micros()
                        as i128 as f64
                        - total_remaining; //计算任务可用时间

                    // TODO:确认一下多线程的ava_time是否正确
                    self.scheduler
                        .update_ava_time(worker_id, co_stat.get_co_id(), available_time);
                    // if let Ok(map) = AVA_TIME.lock().as_mut() {
                    //     map.insert(co_stat.get_co_id(), available_time);
                    // }

                    return SchedulabilityResult {
                        ac: AdmissionControl::SCHEDULABLE,
                        worker_id: Some(worker_id),
                        costatus: Some(co_stat.clone()),
                    };
                }
            }

            //如果𝑑_𝑛𝑒𝑤 - 𝑑_𝑙𝑎𝑠𝑡 < 𝐶_𝑛𝑒𝑤
            stat_vec.push(co_stat); //将所判断的任务的状态信息 co_stat 放入堆中
            let s1 = stat_vec.peek().unwrap().to_owned(); //查看堆中的第一个元素，即具有最早截止日期的任务。
            let mut found_task = false; //标志是否在二叉堆里找到指定任务

            // pop更高优先级的任务
            while let Some(s) = stat_vec.pop() {
                if !found_task && s == co_stat {
                    found_task = true;
                }
                if !found_task {
                    if s.absolute_deadline.is_some() {
                        total_remaining +=
                            s.expected_remaining_execution_time.unwrap().as_micros() as i128 as f64;
                    }
                }
                if found_task {
                    break;
                }
            }

            // d_new - t - RC >= C_new
            let available_time = (co_stat.absolute_deadline.unwrap() - now).as_micros() as i128
                as f64
                - total_remaining; //计算任务可用时间
            if available_time
                < co_stat
                    .expected_remaining_execution_time
                    .unwrap()
                    .as_micros() as i128 as f64
            {
                return SchedulabilityResult {
                    ac: AdmissionControl::UNSCHEDULABLE,
                    worker_id: None,
                    costatus: None,
                };
            } else {
                // TODO:这里需要更新AVA_TIME吗？先注释了
                // self.scheduler
                //     .update_ava_time(worker_id, co_stat.get_co_id(), available_time);
                // if let Ok(map) = AVA_TIME.lock().as_mut() {
                //     map.insert(co_stat.get_co_id(), available_time);
                // }
            }

            // TODO:确认是否已经pop了？先注释了
            // stat_vec.pop(); //弹出co_stat

            // 继续验证低优先级任务
            if let Some(mut map) = self.scheduler.get_ava_time(worker_id) {
                //先复制AVA_TIME的状态
                while let Some(s) = stat_vec.pop() {
                    //验证后面的任务是否满足
                    // time_i - C_new >= C_i
                    if s.absolute_deadline > co_stat.absolute_deadline {
                        let time = map.get(&s.get_co_id());
                        if let Some(time) = time {
                            if (time
                                - (co_stat.expected_remaining_execution_time.unwrap()).as_micros()
                                    as i128 as f64)
                                < (s.expected_remaining_execution_time.unwrap()).as_micros() as i128
                                    as f64
                            {
                                // 不可调度，无需改变AVA_TIME的状态
                                return SchedulabilityResult {
                                    ac: AdmissionControl::UNSCHEDULABLE,
                                    worker_id: None,
                                    costatus: None,
                                };
                            } else {
                                //改变后面任务的可用时间
                                map.insert(
                                    s.get_co_id(),
                                    time - co_stat
                                        .expected_remaining_execution_time
                                        .unwrap()
                                        .as_micros()
                                        as f64,
                                );
                            }
                        }
                    }
                }
                // 循环结束更新整个AVA_TIME的状态
                self.scheduler.update_ava_time_map(worker_id, map);
            }

            // while let Some(s) = stat_vec.pop() {
            //     //验证后面的任务是否满足
            //     if s.absolute_deadline > co_stat.absolute_deadline {
            //         if let Ok(mut map) = AVA_TIME.lock() {
            //             //先备份 AVA_TIME 的状态
            //             // TODO: 在循环中备份状态是否有问题？
            //             let backup_ava_time = map.clone();
            //             let time = map.get(&s.get_co_id()).cloned();
            //             if let Some(time) = time {
            //                 if (time
            //                     - (co_stat.expected_remaining_execution_time.unwrap()).as_micros()
            //                         as i128 as f64)
            //                     < (s.expected_remaining_execution_time.unwrap()).as_micros() as i128
            //                         as f64
            //                 {
            //                     *map = backup_ava_time;
            //                     return SchedulabilityResult {
            //                         ac: AdmissionControl::UNSCHEDULABLE,
            //                         worker_id: None,
            //                         costatus: None,
            //                     };
            //                 } else {
            //                     //改变后面任务的可用时间
            //                     map.insert(
            //                         s.get_co_id(),
            //                         time - co_stat
            //                             .expected_remaining_execution_time
            //                             .unwrap()
            //                             .as_micros()
            //                             as f64,
            //                     );
            //                 }
            //             }
            //         }
            //     }
            // }

            // 如果第一个任务恰好是这个任务，即最早截止时间
            if s1.eq(&co_stat) {
                return SchedulabilityResult {
                    ac: AdmissionControl::PREEMPTIVE,
                    worker_id: Some(worker_id),
                    costatus: Some(co_stat.clone()),
                };
            }
            break;
        }

        //后面所有任务验证完再返回可调度
        return SchedulabilityResult {
            ac: AdmissionControl::SCHEDULABLE,
            worker_id: Some(worker_id),
            costatus: Some(co_stat.clone()),
        };
    }

    /**
     * 通过id获取任务状态
     */
    pub fn get_status_by_id(&self, id: u64) -> Option<SchedulerStatus> {
        self.scheduler.get_status_by_id(id)
    }

    /**
     * 获取所有正在执行或就绪任务的状态
     */
    pub fn get_status(&self) -> Option<BTreeMap<u64, SchedulerStatus>> {
        let mut status = BTreeMap::new();
        for id in 0..self.threads.len() {
            if let Some(mut map) = self.scheduler.get_status(id as u8) {
                status.append(&mut map);
            }
        }
        Some(status)
    }

    /**
     * 获取已完成任务的状态
     */
    pub fn get_completed_status(&self) -> Option<BTreeMap<u64, SchedulerStatus>> {
        self.scheduler.get_completed_status()
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        while let Some(t) = self.threads.pop() {
            t.join().unwrap();
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
pub enum AdmissionControl {
    NOTREALTIME,
    PREEMPTIVE,
    SCHEDULABLE,
    UNSCHEDULABLE,
}

/// 准入控制的结果
pub struct SchedulabilityResult {
    ac: AdmissionControl,
    worker_id: Option<u8>,
    costatus: Option<SchedulerStatus>,
}

impl SchedulabilityResult {
    pub fn get_ac(&self) -> AdmissionControl {
        self.ac
    }
}
