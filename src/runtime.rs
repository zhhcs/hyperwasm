use crate::{
    scheduler::Scheduler,
    task::{Coroutine, SchedulerStatus},
    StackSize,
};
use std::{
    collections::BinaryHeap,
    panic::{self, AssertUnwindSafe},
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant},
};

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
                AdmissionControll::PREEMPTIVE => {
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
                }
                AdmissionControll::SCHEDULABLE => {
                    if let Ok(()) = self.scheduler.push(co, true) {
                        self.scheduler.update_status(id, stat);
                    } else {
                        tracing::error!("spawn failed");
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "spawn failed",
                        ));
                    };
                }
                AdmissionControll::UNSCHEDULABLE => {
                    tracing::warn!("id = {} spawn failed, cause: UNSCHEDULABLE", co.get_co_id());
                    self.scheduler.cancell(co);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "spawn failed",
                    ));
                }
            };
        }
        Ok(id)
    }

    fn is_schedulable(&self, co_stat: &SchedulerStatus) -> AdmissionControll {
        while let Some(mut status_map) = self.scheduler.get_status() {
            if status_map.is_empty() {
                // tracing::info!("case 1");
                return AdmissionControll::SCHEDULABLE;
            }
            let curr: u64 = self.scheduler.get_curr_running_id();

            let running = status_map.get(&curr);
            if running.is_none() {
                drop(status_map);
                continue;
            }
            let start = running.unwrap().curr_start_time.unwrap();
            let now = Instant::now();
            if status_map.get(&curr).unwrap().absolute_deadline.is_some() {
                status_map.entry(curr).and_modify(|curr_stat| {
                    let mut eret = curr_stat.expected_remaining_execution_time.unwrap();
                    let time_diff = now - start;
                    if eret > time_diff {
                        eret -= time_diff;
                        curr_stat.expected_remaining_execution_time = Some(eret);
                    } else {
                        curr_stat.expected_remaining_execution_time =
                            Some(std::time::Duration::from_millis(0));
                    }
                });
            } else {
                // tracing::info!("case 2");
                return AdmissionControll::PREEMPTIVE;
            }

            let mut stat_vec = BinaryHeap::new();
            stat_vec.push(co_stat);
            status_map.iter().for_each(|(_, s)| {
                if s.absolute_deadline.is_some() {
                    stat_vec.push(s)
                }
            });

            let mut total_remaining: f64 = 0.0;
            for s in stat_vec.iter() {
                if s.absolute_deadline.is_some() {
                    total_remaining +=
                        s.expected_remaining_execution_time.unwrap().as_micros() as i128 as f64;
                    let deadline =
                        (s.absolute_deadline.unwrap() - start).as_micros() as i128 as f64;
                    let util = total_remaining / deadline;
                    if util > 1.0 {
                        // tracing::info!("case 3");
                        return AdmissionControll::UNSCHEDULABLE;
                    }
                }
            }

            if stat_vec.peek().unwrap().eq(&co_stat) {
                // tracing::info!("case 4");
                return AdmissionControll::PREEMPTIVE;
            }
            break;
        }
        // tracing::info!("case 5");
        AdmissionControll::SCHEDULABLE
    }

    pub fn print_completed_status(&self) {
        let s = self.scheduler.get_completed_status().unwrap();
        s.iter().for_each(|(id, stat)| {
            tracing::info!("id: {}, status: \n{}", id, stat);
        });
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        while let Some(t) = self.threads.pop() {
            t.join().unwrap();
        }
    }
}

enum AdmissionControll {
    PREEMPTIVE,
    SCHEDULABLE,
    UNSCHEDULABLE,
}
