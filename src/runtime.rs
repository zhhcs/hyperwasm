use anyhow::Error;

use crate::{
    scheduler::Scheduler,
    task::{Coroutine, SchedulerStatus},
    StackSize,
};
use std::{
    collections::{BTreeMap, BinaryHeap, HashMap},
    panic::{self, AssertUnwindSafe},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};
lazy_static::lazy_static! {
    static ref AVA_TIME: Arc<Mutex<HashMap<u64, f64>>> = Arc::new(Mutex::new(HashMap::new()));
}

pub struct Runtime {
    scheduler: Arc<Scheduler>,
    threads: Vec<JoinHandle<()>>,
}

impl Default for Runtime {
    fn default() -> Self {
        let scheduler = Scheduler::new(1);
        let threads = Scheduler::start(&scheduler);
        Self { scheduler, threads }
    }
}

impl Runtime {
    pub fn new(worker_threads: Option<u8>) -> Runtime {
        let scheduler = Scheduler::new(worker_threads.unwrap_or_default());
        let threads = Scheduler::start(&scheduler);
        Runtime { scheduler, threads }
    }

    /// 准入控制的结果
    ///
    /// @return
    /// (AdmissionControl, worker_id, SchedulerStatus)
    pub fn admission_control_result(
        &self,
        expected_execution_time: Option<Duration>,
        relative_deadline: Option<Duration>,
    ) -> SchedulabilityResult {
        if relative_deadline.is_none() || expected_execution_time.is_none() {
            return SchedulabilityResult {
                ac: AdmissionControl::NOTREALTIME,
                worker_id: None,
                costatus: None,
            };
        }
        let mut co_stat = SchedulerStatus::new(expected_execution_time, relative_deadline);
        let id = crate::task::get_id();
        co_stat.init(id);
        self.is_schedulable(&co_stat)
    }

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
        let func = Box::new(move || {
            let _ = panic::catch_unwind(AssertUnwindSafe(f));
        });
        let ac = schedulability_result.get_ac();
        let worker_id = schedulability_result.worker_id.unwrap_or_default();
        let status = schedulability_result.costatus;
        match ac {
            AdmissionControl::NOTREALTIME => {
                tracing::info!("NOT REAL TIME");
                let co = Coroutine::new(func, StackSize::default(), false, None, None);
                // let stat = co.get_schedulestatus();
                let id = co.get_co_id();
                // 这里的worker_id没用
                if let Ok(()) = self.scheduler.push(co, false, worker_id) {
                    // self.scheduler.update_status(id, stat, worker_id);
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
                self.scheduler.update_status(id, stat, worker_id);
                self.scheduler.set_slots(worker_id, co);

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
                if let Ok(()) = self.scheduler.push(co, true, worker_id) {
                    self.scheduler.update_status(id, stat, worker_id);
                    return Ok(id);
                } else {
                    tracing::error!("spawn failed");
                    return Err(Error::msg("spawn failed"));
                };
            }
            _ => {
                return Err(Error::msg("spawn failed, cause: UNSCHEDULABLE"));
            }
        }
    }

    fn is_schedulable(&self, co_stat: &SchedulerStatus) -> SchedulabilityResult {
        //TODO: 指定一个worker，怎么选？
        let worker_id = (co_stat.get_co_id() % self.threads.len() as u64) as u8;
        while let Some(mut status_map) = self.scheduler.get_status(worker_id) {
            //获取调度器的任务状态信息并进入循环，没有任务状态信息，循环将退出。
            if status_map.is_empty() {
                //如果任务状态信息为空，表示当前没有其他任务在运行，因此可以直接调度新任务。
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
                //如果当前运行的任务有绝对截止日期
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
                // TODO:这里需要更新AVA_TIME吗？
                // self.scheduler
                //     .update_ava_time(worker_id, co_stat.get_co_id(), available_time);
                // if let Ok(map) = AVA_TIME.lock().as_mut() {
                //     map.insert(co_stat.get_co_id(), available_time);
                // }
            }

            // TODO:确认是否已经pop了？
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

            if s1.eq(&co_stat) {
                // tracing::info!("case 4");
                return SchedulabilityResult {
                    ac: AdmissionControl::PREEMPTIVE,
                    worker_id: Some(worker_id),
                    costatus: Some(co_stat.clone()),
                };
            }
            break;
        }

        //后面所有任务验证完再返回可调度
        // tracing::info!("case 5");
        return SchedulabilityResult {
            ac: AdmissionControl::SCHEDULABLE,
            worker_id: Some(worker_id),
            costatus: Some(co_stat.clone()),
        };
    }

    pub fn get_status_by_id(&self, id: u64) -> Option<SchedulerStatus> {
        self.scheduler.get_status_by_id(id)
    }

    pub fn get_status(&self) -> Option<BTreeMap<u64, SchedulerStatus>> {
        self.scheduler.get_status(0)
    }

    // pub fn print_completed_status(&self) {
    //     let s = self.scheduler.get_completed_status().unwrap();
    //     s.iter().for_each(|(id, stat)| {
    //         tracing::info!("id: {}, status: \n{}", id, stat);
    //     });
    // }

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
