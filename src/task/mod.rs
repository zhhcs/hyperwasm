mod context;
mod page_size;
pub mod stack;
use crate::scheduler::Scheduler;

use self::context::{Context, Entry};
use self::stack::StackSize;
use std::cell::{Cell, UnsafeCell};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{fmt, panic};
use std::{mem, ptr};

pub static mut ID: AtomicU64 = AtomicU64::new(1);

fn get_id() -> u64 {
    unsafe { ID.fetch_add(1, Ordering::SeqCst) }
}

thread_local! {
    static COROUTINE: Cell<Option<ptr::NonNull<Coroutine>>> = Cell::new(None);
    static THREAD_CONTEXT: UnsafeCell<Context> = UnsafeCell::new(Context::empty());
}

pub fn current() -> Option<ptr::NonNull<Coroutine>> {
    COROUTINE.with(|cell| cell.get())
}

pub fn current_is_none() -> bool {
    COROUTINE.with(|cell| cell.get().is_none())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CoStatus {
    PENDING = 1,
    READY,
    RUNNING,
    SUSPENDED,
    COMPLETED,
    CANCELLED,
    // TODO!
}

struct Scope {
    co: ptr::NonNull<Coroutine>,
}

impl Scope {
    fn enter(co: &Coroutine) -> Scope {
        COROUTINE.with(|cell| {
            assert!(cell.get().is_none(), "running coroutine not exited");
            cell.set(Some(ptr::NonNull::from(co)));
        });
        Scope {
            co: ptr::NonNull::from(co),
        }
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        // println!("scope dropped");
        COROUTINE.with(|cell| {
            let co = cell.replace(None).expect("no running coroutine");
            assert!(co == self.co, "running coroutine changed");
        })
    }
}

struct ThisThread;

impl ThisThread {
    fn context<'a>() -> &'a Context {
        THREAD_CONTEXT.with(|c| unsafe { &*c.get() })
    }

    fn context_mut<'a>() -> &'a mut Context {
        THREAD_CONTEXT.with(|c| unsafe { &mut *c.get() })
    }

    fn resume(context: &Context) {
        context.switch(Self::context_mut());
    }

    fn suspend(context: &mut Context) {
        Self::context().switch(context);
    }

    fn restore() {
        Self::context().resume();
    }
}

#[derive(Clone, Debug)]
pub struct SchedulerStatus {
    status: BTreeMap<Instant, CoStatus>,
    co_id: u64,
    co_status: CoStatus,
    pub curr_start_time: Option<Instant>,
    running_time: Duration,

    spawn_time: Instant,
    expected_execution_time: Option<Duration>,
    pub expected_remaining_execution_time: Option<Duration>,
    worst_start_time: Option<Instant>,
    relative_deadline: Option<Duration>,
    pub absolute_deadline: Option<Instant>,
}

impl SchedulerStatus {
    fn new(
        expected_execution_time: Option<Duration>,
        relative_deadline: Option<Duration>,
    ) -> SchedulerStatus {
        SchedulerStatus {
            status: BTreeMap::new(),
            co_id: 0,
            co_status: CoStatus::PENDING,
            curr_start_time: None,
            running_time: Duration::from_nanos(0),
            spawn_time: Instant::now(),
            expected_execution_time,
            expected_remaining_execution_time: expected_execution_time,
            worst_start_time: None,
            relative_deadline,
            absolute_deadline: None,
        }
    }

    fn init(&mut self, id: u64) {
        self.co_id = id;
        if let Some(rd) = self.relative_deadline {
            self.absolute_deadline = Some(self.spawn_time + rd);
            self.worst_start_time =
                Some(self.spawn_time + rd - self.expected_execution_time.unwrap());
        }
    }

    fn update_remaining(&mut self) {
        if let Some(eet) = self.expected_execution_time {
            if eet >= self.running_time {
                self.expected_remaining_execution_time = Some(eet - self.running_time);
            } else {
                self.expected_remaining_execution_time = Some(eet);
                println!("panicked at 'overflow when subtracting durations");
            }
        }
    }

    fn update_status(&mut self, now: Instant, stat: CoStatus) {
        self.co_status = stat;
        self.status.insert(now, stat);
    }

    fn update_running_time(&mut self, now: Instant) {
        if let Some(start) = self.curr_start_time {
            self.running_time += now - start;
        }
        self.curr_start_time = None;
    }

    pub fn get_co_id(&self) -> u64 {
        self.co_id
    }
}

impl Ord for SchedulerStatus {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // self.absolute_deadline.cmp(&other.absolute_deadline)
        match self.absolute_deadline.cmp(&other.absolute_deadline) {
            std::cmp::Ordering::Less => std::cmp::Ordering::Greater,
            std::cmp::Ordering::Equal => std::cmp::Ordering::Equal,
            std::cmp::Ordering::Greater => std::cmp::Ordering::Less,
        }
    }
}

impl PartialOrd for SchedulerStatus {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for SchedulerStatus {
    fn eq(&self, other: &Self) -> bool {
        self.absolute_deadline == other.absolute_deadline
    }
}

impl Eq for SchedulerStatus {}

impl fmt::Display for SchedulerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = crate::scheduler::get_start();
        self.status
            .iter()
            .for_each(|(time, stat)| writeln!(f, "{:?}, {:?}", *time - start, stat).unwrap());
        if let Some(deadline) = self.absolute_deadline {
            writeln!(f, "deadline {:?}", deadline - start).unwrap();
        }
        writeln!(f, "running time: {:?}", self.running_time)
    }
}
pub struct Coroutine {
    context: Box<Context>,
    status: CoStatus,
    panicking: Option<&'static str>,
    f: Option<Box<dyn FnOnce()>>,
    id: u64,
    stack_size: StackSize,
    schedule_status: SchedulerStatus,
}

unsafe impl Sync for Coroutine {}
unsafe impl Send for Coroutine {}

impl Coroutine {
    pub fn new(
        f: Box<dyn FnOnce()>,
        stack_size: StackSize,
        thread_local: bool,
        expected_execution_time: Option<Duration>,
        relative_deadline: Option<Duration>,
    ) -> Box<Coroutine> {
        #[allow(invalid_value)]
        let mut co = Box::new(Coroutine {
            f: Option::Some(f),
            context: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            status: CoStatus::PENDING,
            panicking: None,
            id: get_id(),
            stack_size,
            schedule_status: SchedulerStatus::new(expected_execution_time, relative_deadline),
        });
        co.schedule_status
            .update_status(co.schedule_status.spawn_time, CoStatus::PENDING);
        co.schedule_status.init(co.id);
        if thread_local {
            let entry = Entry {
                f: Self::main,
                arg: (co.as_mut() as *mut Coroutine) as *mut libc::c_void,
                stack_size,
            };
            mem::forget(mem::replace(&mut co.context, Context::new(&entry, None)));
            co.status = CoStatus::READY;
            co.schedule_status
                .update_status(Instant::now(), CoStatus::READY);
        }
        co
    }

    pub fn set_status(&mut self, status: CoStatus) {
        self.status = status;
    }

    pub fn get_status(&self) -> CoStatus {
        self.status
    }

    pub fn init(&mut self) {
        let entry = Entry {
            f: Self::main,
            arg: (self as *mut Coroutine) as *mut libc::c_void,
            stack_size: self.stack_size,
        };
        mem::forget(mem::replace(&mut self.context, Context::new(&entry, None)));
        self.set_status(CoStatus::READY);
        let now = Instant::now();
        self.schedule_status.update_status(now, self.status);
    }

    extern "C" fn main(arg: *mut libc::c_void) {
        let co = unsafe { &mut *(arg as *mut Coroutine) };
        co.run();
        co.status = CoStatus::COMPLETED;
        let now = Instant::now();
        co.schedule_status.update_running_time(now);
        co.schedule_status.update_remaining();
        co.schedule_status.update_status(now, CoStatus::COMPLETED);
        ThisThread::restore();
    }

    fn run(&mut self) {
        let f = self.f.take().expect("no entry function");
        f();
    }

    // pub fn set_panic(&mut self, msg: &'static str) {
    //     self.panicking = Some(msg);
    // }

    /// Resumes coroutine.
    pub fn resume(&mut self, sched: &Arc<Scheduler>) -> bool {
        // println!("start resume");
        let now = Instant::now();
        self.schedule_status.curr_start_time = Some(now);
        self.status = CoStatus::RUNNING;
        self.schedule_status.update_status(now, self.status);
        sched.update_status(self.get_co_id(), self.get_schedulestatus());
        sched.set_curr_running_id(self.get_co_id());

        let _scope = Scope::enter(self);

        ThisThread::resume(&self.context);

        match self.status {
            CoStatus::COMPLETED => false,
            _ => true,
        }
    }

    pub fn suspend(&mut self, sched: &Arc<Scheduler>) {
        // println!("start suspend");
        let now = Instant::now();
        self.schedule_status.update_status(now, self.status);
        self.schedule_status.update_running_time(now);
        self.schedule_status.update_remaining();
        sched.update_status(self.get_co_id(), self.get_schedulestatus());
        ThisThread::suspend(&mut self.context);
        if let Some(msg) = self.panicking {
            panic::panic_any(msg);
        }
    }

    pub fn get_co_id(&self) -> u64 {
        self.id
    }

    pub fn get_schedulestatus(&self) -> SchedulerStatus {
        self.schedule_status.clone()
    }

    pub fn is_realtime(&self) -> bool {
        self.schedule_status.absolute_deadline.is_some()
    }

    // pub fn set_no_realtime(&mut self) {
    //     self.schedule_status.absolute_deadline = None;
    // }
}

impl Ord for Coroutine {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.schedule_status.cmp(&other.schedule_status)
    }
}

impl PartialOrd for Coroutine {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Coroutine {
    fn eq(&self, other: &Self) -> bool {
        self.schedule_status == other.schedule_status
    }
}

impl Eq for Coroutine {}
