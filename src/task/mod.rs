mod context;
mod page_size;
pub(crate) mod stack;
use self::context::{Context, Entry};
use self::stack::StackSize;
use std::cell::{Cell, UnsafeCell};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
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

pub(crate) fn current() -> Option<ptr::NonNull<Coroutine>> {
    COROUTINE.with(|cell| cell.get())
}

pub(crate) fn current_is_none() -> bool {
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
pub(crate) struct SchedulerStatus {
    status: BTreeMap<Instant, CoStatus>,
    curr_start_time: Option<Instant>,
    running_time: Duration,
}

impl SchedulerStatus {
    fn new() -> SchedulerStatus {
        SchedulerStatus {
            status: BTreeMap::new(),
            curr_start_time: None,
            running_time: Duration::from_nanos(0),
        }
    }

    fn update_status(&mut self, now: Instant, stat: CoStatus) {
        self.status.insert(now, stat);
    }

    fn update_running_time(&mut self, now: Instant) {
        if let Some(start) = self.curr_start_time {
            self.running_time += now - start;
        }
        self.curr_start_time = None;
    }
}

impl fmt::Display for SchedulerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start = crate::scheduler::get_start();
        self.status
            .iter()
            .for_each(|(time, stat)| writeln!(f, "{:?}, {:?}", *time - start, stat).unwrap());
        writeln!(f, "running time: {:?}", self.running_time)
    }
}
pub(crate) struct Coroutine {
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
    pub(crate) fn new(
        f: Box<dyn FnOnce()>,
        stack_size: StackSize,
        thread_local: bool,
    ) -> Box<Coroutine> {
        #[allow(invalid_value)]
        let mut co = Box::new(Coroutine {
            f: Option::Some(f),
            context: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            status: CoStatus::PENDING,
            panicking: None,
            id: get_id(),
            stack_size,
            schedule_status: SchedulerStatus::new(),
        });
        co.schedule_status
            .update_status(Instant::now(), CoStatus::PENDING);
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

    pub(crate) fn set_status(&mut self, status: CoStatus) {
        self.status = status;
    }

    pub fn get_status(&self) -> CoStatus {
        self.status
    }

    pub(crate) fn init(&mut self) {
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
    pub(crate) fn resume(&mut self) -> bool {
        // println!("start resume");
        let now = Instant::now();
        self.schedule_status.curr_start_time = Some(now);
        self.status = CoStatus::RUNNING;
        self.schedule_status.update_status(now, self.status);

        let _scope = Scope::enter(self);

        ThisThread::resume(&self.context);

        match self.status {
            CoStatus::COMPLETED => false,
            _ => true,
        }
    }

    pub(crate) fn suspend(&mut self) {
        // println!("start suspend");
        let now = Instant::now();
        self.schedule_status.update_status(now, self.status);
        self.schedule_status.update_running_time(now);
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
}
