pub mod runtime;
pub mod scheduler;
pub mod task;

use std::sync::Arc;

use runtime::Runtime;
pub use task::stack::StackSize;

static mut TIMER: Option<Timer> = None;

fn main() {
    let rt = Arc::new(Runtime::new());
    let t = set_timer();
    unsafe { TIMER = Some(t) };
    for i in 1..10 {
        rt.spawn(Box::new(move || {
            let i = i.clone();
            do_something(i);
        }))
        .unwrap();
        // println!("Task {} spawned", i);
    }
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

fn do_something(i: i32) {
    println!("Task {} start", i);
    let mut num = 35;
    for index in 0..i % 5 {
        num += index;
        let res = fib(num);
        if res > fib(35) {
            num -= 2;
            // println!("this is task {}, res = {}", i, res % fib(i));
        }
    }
    println!("Task {} end", i);
}

use nix::sys::signal::{self, SigEvent, SigHandler, SigevNotify, Signal};
use nix::sys::timer::{Expiration, Timer, TimerSetTimeFlags};
use nix::time::ClockId;
use std::convert::TryFrom;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::scheduler::TID;

const SIG: Signal = Signal::SIGURG;

extern "C" fn handle_alarm(signal: libc::c_int) {
    let signal = Signal::try_from(signal).unwrap();
    if signal == SIG {
        let mut mask: libc::sigset_t = unsafe { std::mem::zeroed() };
        unsafe {
            libc::sigfillset(&mut mask);
            libc::sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut());
        }
        let worker = unsafe { scheduler::worker::get_worker().as_mut() };
        // println!("suspend and resume");
        worker.suspend();
        worker.get_task();

        worker.set_curr();
        unsafe {
            libc::sigemptyset(&mut mask);
            libc::sigprocmask(libc::SIG_UNBLOCK, &mask, std::ptr::null_mut());
        }
    }
}

pub fn set_timer() -> Timer {
    while unsafe { TID.load(Ordering::Relaxed) } == 0 {}
    let tid = unsafe { TID.load(Ordering::Relaxed) };
    println!("tid = {}", tid);
    let clockid = ClockId::CLOCK_MONOTONIC;
    let sigevent = SigEvent::new(SigevNotify::SigevThreadId {
        signal: SIG,
        si_value: 0,
        thread_id: tid,
    });

    let mut timer = Timer::new(clockid, sigevent).unwrap();
    let expiration = Expiration::Interval(Duration::from_millis(100).into());
    let flags = TimerSetTimeFlags::empty();
    timer.set(expiration, flags).expect("could not set timer");

    let handler = SigHandler::Handler(handle_alarm);
    unsafe { signal::signal(SIG, handler) }.unwrap();
    timer
}
