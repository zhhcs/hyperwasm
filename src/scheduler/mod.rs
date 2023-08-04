use crate::{
    cgroupv2,
    scheduler::worker::{get_worker, Worker},
    task::{current, Coroutine, SchedulerStatus},
};
use chrono::{DateTime, Local};
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
    collections::{BTreeMap, BinaryHeap, VecDeque},
    convert::TryFrom,
    ptr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub mod worker;
pub const PREEMPTY: Signal = Signal::SIGURG;
pub const SIG: Signal = Signal::SIGALRM;
pub static mut PTHREADTID: nix::sys::pthread::Pthread = 0;

thread_local! {
    static TIMER: Cell<Option<ptr::NonNull<LocalTimer>>> = Cell::new(None);
    static START: Cell<Option<(Instant, DateTime<Local>)>> = Cell::new(None);
}

fn get_timer() -> ptr::NonNull<LocalTimer> {
    TIMER.with(|cell| cell.get()).expect("no timer")
}

pub fn init_start() {
    START.with(|cell| {
        assert!(cell.get().is_none());
        cell.set(Some((Instant::now(), Local::now())));
    });
}

pub fn get_start() -> (Instant, DateTime<Local>) {
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
    fn new(thread_id: i32, expiration: u64, signal: Signal, timer_type: usize) -> LocalTimer {
        // let thread_id = get_thread_id();
        let mut exp = Expiration::Interval(Duration::from_nanos(expiration).into());

        if timer_type == 1 {
            exp = Expiration::OneShot(Duration::from_nanos(expiration).into());
        }
        let timer = Self::set_timer(thread_id, exp, signal);
        LocalTimer {
            timer,
            expiration: exp,
        }
    }

    fn init(&self) {
        let timer = ptr::NonNull::from(self);
        TIMER.with(|t| t.set(Some(timer)));
    }

    fn set_timer(tid: i32, expiration: Expiration, signal: Signal) -> Timer {
        let clockid = ClockId::CLOCK_MONOTONIC;
        let sigevent = SigEvent::new(SigevNotify::SigevThreadId {
            signal,
            si_value: 0,
            thread_id: tid,
        });

        let mut timer = Timer::new(clockid, sigevent).unwrap();
        let flags = TimerSetTimeFlags::empty();
        timer.set(expiration, flags).expect("could not set timer");

        let handler = SigHandler::Handler(signal_handler);
        unsafe { signal::signal(signal, handler) }.unwrap();
        timer
    }

    pub fn reset_timer(&mut self) {
        if self.expiration != Expiration::Interval(Duration::from_nanos(0).into()) {
            let flags = TimerSetTimeFlags::empty();
            self.timer
                .set(self.expiration, flags)
                .expect("could not set timer");
            // tracing::info!("reset timer");
        }
    }
}

pub struct Scheduler {
    slot: Mutex<Option<Box<Coroutine>>>,
    realtime_queue: Mutex<BinaryHeap<Box<Coroutine>>>,
    global_queue: Mutex<VecDeque<Box<Coroutine>>>,
    cancelled_queue: Mutex<VecDeque<Box<Coroutine>>>,
    co_status: Mutex<BTreeMap<u64, SchedulerStatus>>,
    completed_status: Mutex<BTreeMap<u64, SchedulerStatus>>,
    curr_running_id: AtomicU64,
}

unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}

impl Scheduler {
    pub fn new() -> Arc<Scheduler> {
        Arc::new(Scheduler {
            slot: Mutex::new(None),
            realtime_queue: Mutex::new(BinaryHeap::new()),
            global_queue: Mutex::new(VecDeque::new()),
            cancelled_queue: Mutex::new(VecDeque::new()),
            co_status: Mutex::new(BTreeMap::new()),
            completed_status: Mutex::new(BTreeMap::new()),
            curr_running_id: AtomicU64::new(0),
        })
    }

    pub fn start(self: &Arc<Scheduler>) -> Vec<JoinHandle<()>> {
        Self::create_cg();
        let scheduler = self.clone();
        init_start();
        let t = thread::spawn(move || {
            let pthreadtid = nix::sys::pthread::pthread_self();
            unsafe { PTHREADTID = pthreadtid };
            let sa = libc::sigaction {
                sa_sigaction: signal_handler as libc::sighandler_t,
                sa_mask: unsafe { std::mem::zeroed() },
                sa_flags: libc::SA_SIGINFO | libc::SA_RESTART,
                sa_restorer: None,
            };
            unsafe {
                libc::sigaction(libc::SIGURG, &sa, std::ptr::null_mut());
            }

            let w = Worker::new(&scheduler, 16);
            let tid = gettid();
            w.set_cgroup(tid);
            w.init();
            let w = unsafe { get_worker().as_mut() };

            // 设置线程定时器
            let timer = LocalTimer::new(tid.into(), 10_000_000, SIG, 3);
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

    pub fn set_slot(&self, co: Box<Coroutine>) {
        while let Ok(slot) = self.slot.try_lock().as_mut() {
            if slot.is_none() {
                let _ = slot.insert(co);
                // tracing::info!("slot inserted {:?}", Instant::now());
                break;
            }
        }
    }

    pub fn get_slot(&self) -> Option<Box<Coroutine>> {
        if let Ok(slot) = self.slot.lock().as_mut() {
            // tracing::info!("try_get_slot");
            slot.take()
        } else {
            None
        }
    }

    pub fn push(&self, co: Box<Coroutine>, realtime: bool) -> Result<(), std::io::Error> {
        if realtime {
            if let Ok(q) = self.realtime_queue.lock().as_mut() {
                q.push(co);
                return Ok(());
            }
        } else {
            if let Ok(q) = self.global_queue.lock().as_mut() {
                q.push_back(co);
                return Ok(());
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "spawn failed",
        ))
    }

    pub fn cancell(&self, co: Box<Coroutine>) {
        if let Ok(q) = self.cancelled_queue.try_lock().as_mut() {
            q.push_back(co);
        }
    }

    pub fn pop_realtime(&self) -> Option<Box<Coroutine>> {
        if let Ok(q) = self.realtime_queue.try_lock().as_mut() {
            q.pop()
        } else {
            None
        }
    }

    pub fn pop(&self) -> Option<Box<Coroutine>> {
        if let Ok(q) = self.global_queue.try_lock().as_mut() {
            q.pop_front()
        } else {
            None
        }
    }

    pub fn get_length(&self) -> usize {
        if let Ok(q) = self.global_queue.try_lock() {
            q.len()
        } else {
            0
        }
    }

    pub fn update_status(&self, co_id: u64, stat: SchedulerStatus) {
        if let Ok(status) = self.co_status.lock().as_mut() {
            status.insert(co_id, stat);
        }
    }

    fn delete_status(&self, co_id: u64) {
        if let Ok(status) = self.co_status.lock().as_mut() {
            status.remove(&co_id);
        }
    }

    pub fn get_status(&self) -> Option<BTreeMap<u64, SchedulerStatus>> {
        if let Ok(status) = self.co_status.try_lock().as_mut() {
            return Some(status.clone());
        }
        None
    }

    pub fn get_status_by_id(&self, id: u64) -> Option<SchedulerStatus> {
        if let Ok(status) = self.co_status.try_lock() {
            if status.contains_key(&id) {
                return status.get(&id).cloned();
            }
        }
        if let Ok(status) = self.completed_status.try_lock() {
            return status.get(&id).cloned();
        }
        None
    }

    pub fn get_completed_status(&self) -> Option<BTreeMap<u64, SchedulerStatus>> {
        if let Ok(status) = self.completed_status.try_lock().as_mut() {
            return Some(status.clone());
        }
        None
    }

    pub fn update_completed_status(&self, co_id: u64, stat: SchedulerStatus) {
        if let Ok(status) = self.completed_status.lock().as_mut() {
            status.insert(co_id, stat);
        }
        self.delete_status(co_id);
    }

    pub fn set_curr_running_id(&self, co_id: u64) {
        self.curr_running_id.store(co_id, Ordering::SeqCst);
    }

    pub fn get_curr_running_id(&self) -> u64 {
        self.curr_running_id.load(Ordering::SeqCst)
    }
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        if let Ok(q) = self.cancelled_queue.try_lock().as_mut() {
            q.iter_mut().for_each(|co| co.init());
        }
    }
}

extern "C" fn signal_handler(signal: libc::c_int) {
    let signal = Signal::try_from(signal).unwrap();
    if signal == PREEMPTY {
        let mut mask: libc::sigset_t = unsafe { std::mem::zeroed() };
        unsafe {
            libc::sigfillset(&mut mask);
            libc::sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut());
        }

        let worker = unsafe { get_worker().as_mut() };

        if worker.preemptive() {
            // let start = Instant::now();
            worker.suspend();
            // let end = Instant::now();
            // tracing::info!("time cost: {:?}", end - start);
        };
        // unsafe { get_timer().as_mut() }.reset_timer();
        // tracing::info!("now suspended {:?}", Instant::now());

        unsafe {
            libc::sigemptyset(&mut mask);
            libc::sigprocmask(libc::SIG_UNBLOCK, &mask, std::ptr::null_mut());
        }
    } else if signal == SIG {
        let mut mask: libc::sigset_t = unsafe { std::mem::zeroed() };
        unsafe {
            libc::sigfillset(&mut mask);
            libc::sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut());
        }

        if let Some(current) = current() {
            if unsafe { !current.as_ref().is_realtime() } {
                let worker = unsafe { get_worker().as_mut() };
                worker.get_task();
                if worker.len > 1 {
                    worker.suspend();
                    worker.set_curr();
                }
            }
        }

        // unsafe { get_timer().as_mut() }.reset_timer();

        unsafe {
            libc::sigemptyset(&mut mask);
            libc::sigprocmask(libc::SIG_UNBLOCK, &mask, std::ptr::null_mut());
        }
    }
}
