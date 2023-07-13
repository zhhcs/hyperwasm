use crate::{
    scheduler::worker::{get_worker, Worker},
    task::Coroutine,
};
use nix::{
    sys::{
        signal::{self, SigEvent, SigHandler, SigevNotify, Signal},
        timer::Timer,
        timer::{Expiration, TimerSetTimeFlags},
    },
    time::ClockId,
    unistd::gettid,
};
use std::{
    cell::Cell,
    collections::VecDeque,
    convert::TryFrom,
    ptr,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

const SIG: Signal = Signal::SIGURG;
pub mod worker;

thread_local! {
    static TIMER: Cell<Option<ptr::NonNull<LocalTimer>>> = Cell::new(None);
}

fn get_timer() -> ptr::NonNull<LocalTimer> {
    TIMER.with(|cell| cell.get()).expect("no timer")
}

struct LocalTimer {
    timer: Timer,
    expiration: Expiration,
}

impl LocalTimer {
    fn new(thread_id: i32, expiration: u64) -> LocalTimer {
        // let thread_id = get_thread_id();
        let expiration = Expiration::Interval(Duration::from_millis(expiration).into());
        let timer = Self::set_timer(thread_id, expiration);
        LocalTimer { timer, expiration }
    }

    fn init(&self) {
        let timer = ptr::NonNull::from(self);
        TIMER.with(|t| t.set(Some(timer)));
    }

    fn set_timer(tid: i32, expiration: Expiration) -> Timer {
        let clockid = ClockId::CLOCK_MONOTONIC;
        let sigevent = SigEvent::new(SigevNotify::SigevThreadId {
            signal: SIG,
            si_value: 0,
            thread_id: tid,
        });

        let mut timer = Timer::new(clockid, sigevent).unwrap();
        let flags = TimerSetTimeFlags::empty();
        timer.set(expiration, flags).expect("could not set timer");

        let handler = SigHandler::Handler(signal_handler);
        unsafe { signal::signal(SIG, handler) }.unwrap();
        timer
    }

    pub(crate) fn reset_timer(&mut self) {
        let flags = TimerSetTimeFlags::empty();
        self.timer
            .set(self.expiration, flags)
            .expect("could not set timer");
        // println!("reset timer");
    }
}

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
            let w = Worker::new(&scheduler, 4);
            w.init();
            let w = unsafe { get_worker().as_mut() };

            let tid = gettid().into();
            let timer = LocalTimer::new(tid, 10);
            timer.init();

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

extern "C" fn signal_handler(signal: libc::c_int) {
    // println!("now {:?}", std::time::Instant::now());
    let signal = Signal::try_from(signal).unwrap();
    if signal == SIG {
        let mut mask: libc::sigset_t = unsafe { std::mem::zeroed() };
        unsafe {
            libc::sigfillset(&mut mask);
            libc::sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut());
        }

        let worker = unsafe { get_worker().as_mut() };
        worker.suspend();
        worker.get_task();
        worker.set_curr();

        unsafe { get_timer().as_mut() }.reset_timer();

        unsafe {
            libc::sigemptyset(&mut mask);
            libc::sigprocmask(libc::SIG_UNBLOCK, &mask, std::ptr::null_mut());
        }
    }
}
