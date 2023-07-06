pub mod task;

use crate::task::{current, Coroutine};
use std::thread;

pub use task::stack::StackSize;

use crate::task::{current_is_none, tasks, TaskQueue};

fn main() {
    let (sender, receiver) = std::sync::mpsc::channel();

    let _t = thread::spawn(move || {
        let tid = nix::sys::pthread::pthread_self();
        sender.send(tid).unwrap();
        let sa = libc::sigaction {
            sa_sigaction: signal_handler as libc::sighandler_t,
            sa_mask: unsafe { std::mem::zeroed() },
            sa_flags: libc::SA_SIGINFO | libc::SA_RESTART,
            sa_restorer: None,
        };

        unsafe {
            libc::sigaction(libc::SIGURG, &sa, std::ptr::null_mut());
        }

        let co1 = Coroutine::new(
            Box::new(|| {
                println!("Task 1 start");
                let id = 1;
                let mut cnt = 36;
                loop {
                    cnt += 1;
                    if fib(cnt) > fib(35) {
                        cnt -= 2;
                        println!("this is task {}", id);
                    }
                }

                println!("Task 1 end");
            }),
            StackSize::default(),
        );
        let co2 = Coroutine::new(
            Box::new(|| {
                println!("Task 2 start");
                let id = 2;
                let mut cnt = 36;
                loop {
                    cnt += 1;
                    if fib(cnt) > fib(35) {
                        cnt -= 2;
                        println!("this is task {}", id);
                    }
                }

                println!("Task 2 end");
            }),
            StackSize::default(),
        );
        let mut co3 = Coroutine::new(
            Box::new(|| {
                println!("Task 3 start");
                let id = 3;
                let mut cnt = 36;
                loop {
                    cnt += 1;
                    if fib(cnt) > fib(35) {
                        cnt -= 2;
                        println!("this is task {}", id);
                    }
                }

                println!("Task 3 end");
            }),
            StackSize::default(),
        );

        let mut queue = TaskQueue::new(*co1);
        queue.init();
        let tasks = unsafe { tasks().as_mut() };
        println!("get tasks");
        let mut cnt = 0;
        tasks.push_back(std::ptr::NonNull::from(Box::leak(co2)));
        co3.resume();
        while current_is_none() {
            // cnt += 1;
            // if cnt % 10 == 0 {
            //     println!("cnt : {}", cnt);
            // }
            let new = unsafe { tasks.pop_front().unwrap().as_mut() };
            new.resume();
        }
    });
    let tid = receiver.recv().unwrap();
    println!("tid = {}", tid);
    thread::sleep(std::time::Duration::from_millis(100));

    loop {
        // 每隔10ms发送SIGURG信号给子线程
        let sigval = libc::sigval {
            sival_ptr: 0 as *mut libc::c_void,
        };
        let ret = unsafe { libc::pthread_sigqueue(tid, libc::SIGURG, sigval) };

        if ret != 0 {
            eprintln!("Failed to send signal to child thread");
        }

        thread::sleep(std::time::Duration::from_millis(100));
    }
    _t.join().unwrap();
}

fn fib(num: i32) -> i32 {
    if num == 0 {
        return 1;
    }
    if num == 1 {
        return 1;
    }
    fib(num - 1) + fib(num - 2)
}

fn signal_handler() {
    // println!("signal_handler");
    let mut mask: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::sigfillset(&mut mask);
        libc::sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut());
    }
    let tasks = unsafe { tasks().as_mut() };
    // println!("get tasks in signal handler");
    // let new = unsafe { tasks.pop_front().unwrap().as_mut() };
    let old = unsafe { current().as_mut() };
    tasks.push_back(old.into());

    // println!("suspend and resume");

    old.suspend();
    // new.resume();

    unsafe {
        libc::sigemptyset(&mut mask);
        libc::sigprocmask(libc::SIG_UNBLOCK, &mask, std::ptr::null_mut());
    }
}
