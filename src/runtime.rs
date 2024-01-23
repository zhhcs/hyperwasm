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

impl Runtime {
    pub fn new() -> Runtime {
        let scheduler = Scheduler::new();
        let threads = Scheduler::start(&scheduler);
        Runtime { scheduler, threads }
    }

    pub fn admission_control_result(
        &self,
        expected_execution_time: Option<Duration>,
        relative_deadline: Option<Duration>,
    ) -> (AdmissionControl, Option<SchedulerStatus>) {
        if relative_deadline.is_none() || expected_execution_time.is_none() {
            return (AdmissionControl::NOTREALTIME, None);
        }
        let mut co_stat = SchedulerStatus::new(expected_execution_time, relative_deadline);
        let id = crate::task::get_id();
        co_stat.init(id);
        (self.is_schedulable(&co_stat), Some(co_stat))
    }

    pub fn micro_process<F, T>(
        &self,
        f: F,
        ac: AdmissionControl,
        status: Option<SchedulerStatus>,
    ) -> Result<u64, Error>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
    {
        let func = Box::new(move || {
            let _ = panic::catch_unwind(AssertUnwindSafe(f));
        });
        match ac {
            AdmissionControl::NOTREALTIME => {
                tracing::info!("NOT REAL TIME");
                let co = Coroutine::new(func, StackSize::default(), false, None, None);
                let stat = co.get_schedulestatus();
                let id = co.get_co_id();
                if let Ok(()) = self.scheduler.push(co, false) {
                    self.scheduler.update_status(id, stat);
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
                self.scheduler.update_status(id, stat);
                self.scheduler.set_slot(co);

                let sigval = libc::sigval {
                    sival_ptr: 0 as *mut libc::c_void,
                };
                let ret = unsafe {
                    libc::pthread_sigqueue(
                        crate::scheduler::PTHREADTID,
                        crate::scheduler::PREEMPTY as i32,
                        sigval,
                    )
                };
                assert!(ret == 0);
                return Ok(id);
            }
            AdmissionControl::SCHEDULABLE => {
                let co = Coroutine::from_status(func, status.unwrap());
                let stat = co.get_schedulestatus();
                let id = co.get_co_id();
                if let Ok(()) = self.scheduler.push(co, true) {
                    self.scheduler.update_status(id, stat);
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

    // #[deprecated]
    pub fn spawn<F, T>(
        &self,
        f: F,
        expected_execution_time: Option<Duration>,
        relative_deadline: Option<Duration>,
    ) -> Result<u64, std::io::Error>
    where
        F: FnOnce() -> T,
        F: Send + 'static,
        T: Send + 'static,
    {
        let func = Box::new(move || {
            let _ = panic::catch_unwind(AssertUnwindSafe(f));
        });
        let co = Coroutine::new(
            func,
            StackSize::default(),
            false,
            expected_execution_time,
            relative_deadline,
        );
        let stat = co.get_schedulestatus();
        let id = co.get_co_id();
        if !co.is_realtime() {
            // tracing::info!("case 0");
            if let Ok(()) = self.scheduler.push(co, false) {
                self.scheduler.update_status(id, stat);
            } else {
                tracing::error!("spawn failed");
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "spawn failed",
                ));
            };
        } else {
            match self.is_schedulable(&stat) {
                AdmissionControl::PREEMPTIVE => {
                    self.scheduler.set_slot(co);

                    let sigval = libc::sigval {
                        sival_ptr: 0 as *mut libc::c_void,
                    };
                    let ret = unsafe {
                        libc::pthread_sigqueue(
                            crate::scheduler::PTHREADTID,
                            crate::scheduler::PREEMPTY as i32,
                            sigval,
                        )
                    };
                    // let now = Instant::now();
                    // tracing::info!("sig {:?}", now);
                    assert!(ret == 0);
                }
                AdmissionControl::SCHEDULABLE => {
                    if let Ok(()) = self.scheduler.push(co, true) {
                        self.scheduler.update_status(id, stat);
                    } else {
                        tracing::error!("spawn failed");
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "spawn failed, unexpected error",
                        ));
                    };
                }
                AdmissionControl::UNSCHEDULABLE => {
                    // tracing::warn!("id = {} spawn failed, cause: UNSCHEDULABLE", co.get_co_id());
                    self.scheduler.cancell(co);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "spawn failed, cause: UNSCHEDULABLE",
                    ));
                }
                AdmissionControl::NOTREALTIME => (),
            };
        }
        Ok(id)
    }

    fn is_schedulable(&self, co_stat: &SchedulerStatus) -> AdmissionControl {
        while let Some(mut status_map) = self.scheduler.get_status() {
            //获取调度器的任务状态信息并进入循环，没有任务状态信息，循环将退出。
            if status_map.is_empty() {
                //如果任务状态信息为空，表示当前没有其他任务在运行，因此可以直接调度新任务。
                // tracing::info!("case 1");
                return AdmissionControl::SCHEDULABLE;
            }
            let curr: u64 = self.scheduler.get_curr_running_id(); //获取当前正在运行的任务的唯一标识符

            let running = status_map.get(&curr); //获取当前运行的任务的状态信息
            if running.is_none() {
                //如果当前没有正在运行的任务，或者没有启动时间信息，则跳过循环并继续。
                drop(status_map);
                continue;
            }
            let start = running.unwrap().curr_start_time;
            if start.is_none() {
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
                        curr_stat.expected_remaining_execution_time =
                            Some(std::time::Duration::from_millis(0));
                    }
                });
            } else {
                //如果当前运行任务没有绝对截止日期，可以被抢占
                // tracing::info!("case 2");
                return AdmissionControl::PREEMPTIVE;
            }
            //如果𝑑_𝑛𝑒𝑤- 𝑑_𝑙𝑎𝑠𝑡≥ 𝐶_𝑛𝑒𝑤，直接准入
            if let Some(end_ddl) = self.scheduler.get_end_ddl() {
                if co_stat.expected_remaining_execution_time.unwrap()
                    <= co_stat.absolute_deadline.unwrap() - end_ddl
                {
                    // tracing::info!("case 3");
                    let mut stat_vec = BinaryHeap::new(); //创建一个二叉堆存储任务的状态信息
                    status_map.iter_mut().for_each(|(_, s)| {
                        //迭代任务状态信息，将具有绝对截止日期的任务状态信息放入堆中。
                        if s.absolute_deadline.is_some() {
                            stat_vec.push(s)
                        }
                    });
                    let mut total_remaining: f64 = 0.0;
                    while let Some(s) = stat_vec.pop() {
                        if s.absolute_deadline.is_some() {
                            total_remaining +=
                                s.expected_remaining_execution_time.unwrap().as_micros() as i128
                                    as f64;
                        }
                    }
                    let available_time = (co_stat.absolute_deadline.unwrap() - now).as_micros()
                        as i128 as f64
                        - total_remaining; //计算任务可用时间
                    if let Ok(map) = AVA_TIME.lock().as_mut() {
                        map.insert(co_stat.get_co_id(), available_time);
                    }
                    return AdmissionControl::SCHEDULABLE;
                }
            }

            let mut stat_vec = BinaryHeap::new(); //创建一个二叉堆存储任务的状态信息
            stat_vec.push(co_stat); //将所判断的任务的状态信息 co_stat 放入堆中
            status_map.iter_mut().for_each(|(_, s)| {
                //迭代任务状态信息，将具有绝对截止日期的任务状态信息放入堆中。
                if s.absolute_deadline.is_some() {
                    stat_vec.push(s)
                }
            });

            let s1 = stat_vec.peek().unwrap().to_owned(); //获取堆中的第一个元素，即具有最早截止日期的任务。
            let mut total_remaining: f64 = 0.0; //任务的总剩余执行时间
                                                /*status_map.iter().for_each(|(_, s)| {
                                                    if s.absolute_deadline < co_stat.absolute_deadline {
                                                        total_remaining += s.expected_remaining_execution_time.unwrap().as_micros() as i128 as f64;
                                                    }
                                                });
                                                let available_time = (co_stat.absolute_deadline.unwrap() - now).as_micros() as i128 as f64 - total_remaining;  //计算任务可用时间
                                                if available_time < co_stat.expected_remaining_execution_time.unwrap().as_micros() as i128 as f64 {
                                                    return AdmissionControll::UNSCHEDULABLE;
                                                }
                                                for (_, s) in status_map.iter() {
                                                    if s.absolute_deadline > co_stat.absolute_deadline {    //验证后面的任务是否满足
                                                        if (s.available_time.unwrap() - co_stat.expected_remaining_execution_time.unwrap()) < s.expected_remaining_execution_time.unwrap() {
                                                            return AdmissionControll::UNSCHEDULABLE;
                                                        }
                                                    }
                                                    return AdmissionControll::SCHEDULABLE;
                                                }*/
            let mut found_task = false; //标志是否在二叉堆里找到指定任务
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
                /*let deadline =
                    (s.absolute_deadline.unwrap() - start).as_micros() as i128 as f64;
                let util = total_remaining / deadline;
                // tracing::info!("tr: {},ddl: {}", total_remaining, deadline);
                if util > 1.0 {
                    // tracing::info!("case 3");
                    return AdmissionControll::UNSCHEDULABLE;
                }*/
            }
            let available_time = (co_stat.absolute_deadline.unwrap() - now).as_micros() as i128
                as f64
                - total_remaining; //计算任务可用时间
            if available_time
                < co_stat
                    .expected_remaining_execution_time
                    .unwrap()
                    .as_micros() as i128 as f64
            {
                return AdmissionControl::UNSCHEDULABLE;
            } else {
                //co_stat.available_time = Some(std::time::Duration::from_micros(available_time as u64));
                if let Ok(map) = AVA_TIME.lock().as_mut() {
                    map.insert(co_stat.get_co_id(), available_time);
                }
            }
            stat_vec.pop(); //弹出co_stat
            while let Some(s) = stat_vec.pop() {
                //验证后面的任务是否满足
                if s.absolute_deadline > co_stat.absolute_deadline {
                    if let Ok(mut map) = AVA_TIME.lock() {
                        //先备份 AVA_TIME 的状态
                        let backup_ava_time = map.clone();
                        let time = map.get(&s.get_co_id()).cloned();
                        if let Some(time) = time {
                            if (time
                                - (co_stat.expected_remaining_execution_time.unwrap()).as_micros()
                                    as i128 as f64)
                                < (s.expected_remaining_execution_time.unwrap()).as_micros() as i128
                                    as f64
                            {
                                *map = backup_ava_time;
                                return AdmissionControl::UNSCHEDULABLE;
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
            }
            //return AdmissionControll::SCHEDULABLE;   //后面所有任务验证完再返回可调度

            if s1.eq(&co_stat) {
                // tracing::info!("case 4");
                return AdmissionControl::PREEMPTIVE;
            }
            break;
        }
        // tracing::info!("case 5");
        AdmissionControl::SCHEDULABLE
    }

    pub fn get_status_by_id(&self, id: u64) -> Option<SchedulerStatus> {
        self.scheduler.get_status_by_id(id)
    }

    pub fn get_status(&self) -> Option<BTreeMap<u64, SchedulerStatus>> {
        self.scheduler.get_status()
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

    pub fn drop_co(&self) {
        self.scheduler.drop_co();
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        while let Some(t) = self.threads.pop() {
            t.join().unwrap();
        }
    }
}

#[derive(PartialEq)]
pub enum AdmissionControl {
    NOTREALTIME,
    PREEMPTIVE,
    SCHEDULABLE,
    UNSCHEDULABLE,
}
