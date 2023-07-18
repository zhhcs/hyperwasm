use crate::{
    cgroupv2,
    scheduler::worker::{get_worker, Worker},
    task::{Coroutine, SchedulerStatus},
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
    collections::{BTreeMap, VecDeque},
    convert::TryFrom,
    ptr,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(crate) mod worker;
const SIG: Signal = Signal::SIGURG;

thread_local! {
    static TIMER: Cell<Option<ptr::NonNull<LocalTimer>>> = Cell::new(None);
    static START: Cell<Option<Instant>> = Cell::new(None);
}

fn get_timer() -> ptr::NonNull<LocalTimer> {
    TIMER.with(|cell| cell.get()).expect("no timer")
}

pub(crate) fn get_start() -> Instant {
    START.with(|cell| cell.get()).expect("no start")
}

struct LocalTimer {
    timer: Timer,
    expiration: Expiration,
}

impl LocalTimer {
    /// create thread local timer
    ///
    /// expiration 0: disable timer
    ///
    /// others: enable timer
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
    co_status: Mutex<BTreeMap<u64, SchedulerStatus>>,
}

unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}

impl Scheduler {
    pub(crate) fn new() -> Arc<Scheduler> {
        Arc::new(Scheduler {
            global_queue: Mutex::new(VecDeque::new()),
            co_status: Mutex::new(BTreeMap::new()),
        })
    }

    pub(crate) fn start(self: &Arc<Scheduler>) -> Vec<JoinHandle<()>> {
        Self::create_cg();
        let scheduler = self.clone();
        START.with(|cell: &Cell<Option<Instant>>| cell.set(Some(Instant::now())));

        let t = thread::spawn(move || {
            let w = Worker::new(&scheduler, 4);
            let tid = gettid();
            w.set_cgroup(tid);
            w.init();
            let w = unsafe { get_worker().as_mut() };

            // 设置线程定时器
            let timer = LocalTimer::new(tid.into(), 5);
            timer.init();

            w.run();
        });

        let mut v = Vec::new();
        v.push(t);
        v
    }

    fn create_cg() {
        let hypersched = cgroupv2::Controllerv2::new(
            std::path::PathBuf::from("/sys/fs/cgroup"),
            String::from("hypersched"),
        );
        hypersched.set_sub_controller(
            vec![
                cgroupv2::ControllerType::CPU,
                cgroupv2::ControllerType::CPUSET,
            ],
            None,
        );
        hypersched.set_cpuset(0, Some(1));
        hypersched.set_cgroup_procs(nix::unistd::gettid());

        let cg_main = cgroupv2::Controllerv2::new(
            std::path::PathBuf::from("/sys/fs/cgroup/hypersched"),
            String::from("main"),
        );
        cg_main.set_threaded();
        cg_main.set_cpuset(0, None);
        cg_main.set_cgroup_threads(nix::unistd::gettid());
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

    pub(crate) fn update_status(&self, co_id: u64, stat: SchedulerStatus) {
        if let Ok(status) = self.co_status.try_lock().as_mut() {
            status.insert(co_id, stat);
        }
    }

    pub fn get_status(&self) -> Option<BTreeMap<u64, SchedulerStatus>> {
        if let Ok(status) = self.co_status.try_lock().as_mut() {
            return Some(status.clone());
        }
        None
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
