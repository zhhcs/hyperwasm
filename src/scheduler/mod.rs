use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicI32, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use nix::unistd::gettid;

use crate::{
    scheduler::worker::{get_worker, Worker},
    task::Coroutine,
};

pub mod worker;

pub static mut TID: AtomicI32 = AtomicI32::new(0);

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
        let scheduler = self.clone();
        let t = thread::spawn(move || {
            let w = Worker::new(&scheduler, 16);
            w.init();
            let w = unsafe { get_worker().as_mut() };

            let tid0 = gettid().into();
            let _ = unsafe { TID.compare_exchange(0, tid0, Ordering::Acquire, Ordering::Relaxed) };

            w.run();
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
