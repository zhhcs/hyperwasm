use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
};

use crate::{
    scheduler::{
        signal_handler,
        worker::{get_worker, Worker},
    },
    task::Coroutine,
    StackSize,
};

pub(crate) struct Runtime {
    global_queue: Mutex<VecDeque<Box<Coroutine>>>,
    // threadId: usize,
    thread: Option<JoinHandle<()>>,
}

impl Runtime {
    pub fn new() -> Arc<Runtime> {
        Arc::new(Runtime {
            global_queue: Mutex::new(VecDeque::new()),
            thread: None,
        })
    }

    pub fn spawn(&self, f: Box<dyn FnOnce()>) {
        let co = Coroutine::new(Box::new(move || f()), StackSize::default());
        if let Ok(mut q) = self.global_queue.try_lock() {
            q.push_back(co);
        }
    }

    pub fn queue_len(&self) -> usize {
        if let Ok(q) = self.global_queue.try_lock().as_mut() {
            return q.len();
        }
        0
    }

    pub fn take(&self) -> Option<Box<Coroutine>> {
        if let Ok(q) = self.global_queue.try_lock().as_mut() {
            let co = q.pop_front();
            if let Some(co) = co {
                println!("take coroutine id = {} from global queue", co.get_co_id());
                return Some(co);
            }
        }
        None
    }

    pub fn start(self: Arc<Runtime>) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let _t = thread::spawn(move || {
            let w = Worker::new(self.clone(), 16);
            w.init();
            let w = unsafe { get_worker().as_mut() };

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
            w.sched();
        });

        let tid = receiver.recv().unwrap();
        println!("tid = {}", tid);
        thread::sleep(std::time::Duration::from_millis(100));

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
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
    }
}
