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

pub(crate) mod worker;
const SIG: Signal = Signal::SIGURG;
const PREEMPTY: Signal = Signal::SIGALRM;
static mut EPOCH: AtomicU64 = AtomicU64::new(0);

fn epoch() {
    unsafe {
        EPOCH.fetch_add(1, Ordering::SeqCst);
    }
}

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

    pub(crate) fn reset_timer(&mut self) {
        let flags = TimerSetTimeFlags::empty();
        self.timer
            .set(self.expiration, flags)
            .expect("could not set timer");
        // println!("reset timer");
    }
}

pub(crate) struct Scheduler {
    realtime_queue: Mutex<BinaryHeap<Box<Coroutine>>>,
    global_queue: Mutex<VecDeque<Box<Coroutine>>>,
    co_status: Mutex<BTreeMap<u64, SchedulerStatus>>,
    curr_running_id: AtomicU64,
}

unsafe impl Send for Scheduler {}
unsafe impl Sync for Scheduler {}

impl Scheduler {
    pub(crate) fn new() -> Arc<Scheduler> {
        Arc::new(Scheduler {
            realtime_queue: Mutex::new(BinaryHeap::new()),
            global_queue: Mutex::new(VecDeque::new()),
            co_status: Mutex::new(BTreeMap::new()),
            curr_running_id: AtomicU64::new(0),
        })
    }

    pub(crate) fn start(self: &Arc<Scheduler>) -> Vec<JoinHandle<()>> {
        Self::create_cg();
        let scheduler = self.clone();
        START.with(|cell: &Cell<Option<Instant>>| cell.set(Some(Instant::now())));
        // let (sender, receiver) = std::sync::mpsc::channel();
        let t = thread::spawn(move || {
            let w = Worker::new(&scheduler, 16);
            let tid = gettid();
            w.set_cgroup(tid);
            w.init();
            let w = unsafe { get_worker().as_mut() };

            // 设置线程定时器
            let timer = LocalTimer::new(tid.into(), 10_000_000, SIG, 3);
            timer.init();
            // sender.send(tid).unwrap();
            w.run();
        });

        // let scheduler = self.clone();
        // thread::spawn(move || {
        //     let cg_worker = crate::cgroupv2::Controllerv2::new(
        //         std::path::PathBuf::from("/sys/fs/cgroup/hypersched"),
        //         String::from("monitor"),
        //     );
        //     cg_worker.set_threaded();
        //     cg_worker.set_cpuset(0, None);
        //     cg_worker.set_cgroup_threads(gettid());

        //     let tid = receiver.recv().unwrap().into();
        //     thread::sleep(Duration::from_millis(10));
        //     scheduler.check_realtime(tid);
        // });

        let mut v = Vec::new();
        v.push(t);
        v
    }

    pub(crate) fn check_realtime(&self, tid: i32) {
        let mut cnt = 0;
        loop {
            let ep = unsafe { EPOCH.load(Ordering::SeqCst) };
            if cnt != ep {
                if let Ok(q) = self.realtime_queue.try_lock() {
                    if let Ok(stat) = self.co_status.try_lock() {
                        let stat = stat
                            .get(&self.curr_running_id.load(Ordering::SeqCst))
                            .unwrap();
                        if let Some(co) = q.peek() {
                            if co.get_schedulestatus().cmp(stat).is_gt() {
                                let timer = LocalTimer::new(tid, 1000, PREEMPTY, 1);
                                thread::sleep(Duration::from_nanos(1000));
                                drop(timer);
                            }
                        }
                    }
                }
                cnt = ep;
            }
        }
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

    pub(crate) fn push(&self, co: Box<Coroutine>, realtime: bool) -> Result<(), std::io::Error> {
        if realtime {
            if let Ok(q) = self.realtime_queue.try_lock().as_mut() {
                q.push(co);
                epoch();
                return Ok(());
            }
        } else {
            if let Ok(q) = self.global_queue.try_lock().as_mut() {
                q.push_back(co);
                return Ok(());
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "spawn failed",
        ))
    }

    pub(crate) fn pop_realtime(&self) -> Option<Box<Coroutine>> {
        if let Ok(q) = self.realtime_queue.try_lock().as_mut() {
            q.pop()
        } else {
            None
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

    pub(crate) fn set_curr_running_id(&self, co_id: u64) {
        self.curr_running_id.store(co_id, Ordering::SeqCst);
    }
}

extern "C" fn signal_handler(signal: libc::c_int) {
    // println!("now {:?}", std::time::Instant::now());
    let signal = Signal::try_from(signal).unwrap();
    if signal == PREEMPTY {
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
    } else if signal == SIG {
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
