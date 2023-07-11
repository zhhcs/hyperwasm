use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use crate::{
    scheduler::worker::{get_worker, Worker},
    task::Coroutine,
};

pub mod worker;

pub(crate) struct Scheduler {
    global_queue: Mutex<VecDeque<Box<Coroutine>>>,
}

unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}

impl Scheduler {
    pub(crate) fn new() -> Arc<Scheduler> {
        Arc::new(Scheduler {
            global_queue: Mutex::new(VecDeque::new()),
        })
    }

    pub(crate) fn start(self: &Arc<Scheduler>) -> Vec<JoinHandle<()>> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let scheduler = self.clone();
        let t = thread::spawn(move || {
            let w = Worker::new(&scheduler, 16);
            w.init();
            let w = unsafe { get_worker().as_mut() };
            w.spawn_local(Box::new(move || {
                println!("Local Task start");
                let mut cnt = 36;
                for _ in 0..6 {
                    cnt += 1;
                    if crate::fib(cnt) > crate::fib(35) {
                        cnt -= 2;
                        println!("this is local task");
                    }
                }
                println!("local task end");
            }));
            // w.get_task();

            let sa = libc::sigaction {
                sa_sigaction: signal_handler as libc::sighandler_t,
                sa_mask: unsafe { std::mem::zeroed() },
                sa_flags: libc::SA_SIGINFO | libc::SA_RESTART,
                sa_restorer: None,
            };

            unsafe {
                libc::sigaction(libc::SIGURG, &sa, std::ptr::null_mut());
            }

            let tid = nix::sys::pthread::pthread_self();
            sender.send(tid).unwrap();
            w.run();
        });

        let _timer = thread::spawn(move || {
            let tid = receiver.recv().unwrap();
            println!("tid = {}", tid);
            // thread::sleep(std::time::Duration::from_millis(100));
            loop {
                // 每隔100ms发送SIGURG信号给子线程
                let sigval = libc::sigval {
                    sival_ptr: 0 as *mut libc::c_void,
                };
                let ret = unsafe { libc::pthread_sigqueue(tid, libc::SIGURG, sigval) };

                if ret != 0 {
                    eprintln!("Failed to send signal to child thread");
                }

                thread::sleep(std::time::Duration::from_millis(100));
            }
        });
        let mut v = Vec::new();
        v.push(t);
        v
    }

    pub(crate) fn push(&self, co: Box<Coroutine>) -> Result<(), std::io::Error> {
        if let Ok(q) = self.global_queue.try_lock().as_mut() {
            q.push_back(co);
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "spawn failed",
            ))
        }
    }

    pub(crate) fn pop(&self) -> Option<Box<Coroutine>> {
        if let Ok(q) = self.global_queue.try_lock().as_mut() {
            q.pop_front()
        } else {
            None
        }
    }

    pub(crate) fn get_length(&self) -> usize {
        if let Ok(q) = self.global_queue.try_lock() {
            q.len()
        } else {
            0
        }
    }
}

pub(crate) fn signal_handler() {
    // println!("signal_handler");
    let mut mask: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::sigfillset(&mut mask);
        libc::sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut());
    }
    // println!("get local queue in signal handler");

    let worker = unsafe { get_worker().as_mut() };
    // println!("suspend and resume");
    worker.suspend();
    worker.get_task();

    worker.set_curr();

    // println!("########### end of signal handler ##########");
    unsafe {
        libc::sigemptyset(&mut mask);
        libc::sigprocmask(libc::SIG_UNBLOCK, &mask, std::ptr::null_mut());
    }
}
