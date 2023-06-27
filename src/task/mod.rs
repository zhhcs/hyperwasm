mod context;
mod page_size;
pub(crate) mod stack;

use std::cell::{Cell, UnsafeCell};
use std::collections::VecDeque;
use std::panic;

use std::{mem, ptr};

use self::context::{Context, Entry};
use self::stack::StackSize;

thread_local! {
    static COROUTINE: Cell<Option<ptr::NonNull<Coroutine>>> = Cell::new(None);
    static THREAD_CONTEXT: UnsafeCell<Context> = UnsafeCell::new(Context::empty());
    static TASKS: Cell<Option<ptr::NonNull<VecDeque<ptr::NonNull<Coroutine>>>>> = Cell::new(None);
}

pub(crate) fn current() -> ptr::NonNull<Coroutine> {
    COROUTINE.with(|p| p.get()).expect("no running coroutine")
}

pub(crate) fn current_is_none() -> bool {
    COROUTINE.with(|cell| cell.get().is_none())
}

pub(crate) fn tasks() -> ptr::NonNull<VecDeque<ptr::NonNull<Coroutine>>> {
    TASKS.with(|t| t.get()).expect("no running tasks")
}

pub(crate) struct TaskQueue {
    tasks: VecDeque<ptr::NonNull<Coroutine>>,
    co: Coroutine,
}

impl TaskQueue {
    pub fn new(co: Coroutine) -> Self {
        TaskQueue {
            tasks: VecDeque::new(),
            co,
        }
    }

    pub fn init(&mut self) {
        let co = ptr::NonNull::from(&self.co);
        self.tasks.push_back(co);
        let tasks = ptr::NonNull::from(&self.tasks);
        TASKS.with(|t| t.set(Some(tasks)));
    }
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

pub(crate) struct Coroutine {
    context: Box<Context>,
    completed: bool,
    panicking: Option<&'static str>,
    f: Option<Box<dyn FnOnce()>>,
}

unsafe impl Sync for Coroutine {}

impl Coroutine {
    pub fn new(f: Box<dyn FnOnce()>, stack_size: StackSize) -> Box<Coroutine> {
        #[allow(invalid_value)]
        let mut co = Box::new(Coroutine {
            f: Option::Some(f),
            context: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            completed: false,
            panicking: None,
        });
        let entry = Entry {
            f: Self::main,
            arg: (co.as_mut() as *mut Coroutine) as *mut libc::c_void,
            stack_size,
        };
        mem::forget(mem::replace(&mut co.context, Context::new(&entry, None)));
        co
    }

    extern "C" fn main(arg: *mut libc::c_void) {
        let co = unsafe { &mut *(arg as *mut Coroutine) };
        co.run();
        co.completed = true;
        ThisThread::restore();
    }

    fn run(&mut self) {
        let f = self.f.take().expect("no entry function");
        f();
    }

    pub fn set_panic(&mut self, msg: &'static str) {
        self.panicking = Some(msg);
    }

    /// Resumes coroutine.
    ///
    /// Returns whether this coroutine should be resumed again.
    pub fn resume(&mut self) -> bool {
        println!("start resume");

        let _scope = Scope::enter(self);

        ThisThread::resume(&self.context);
        !self.completed
    }

    pub fn suspend(&mut self) {
        println!("start suspend");
        ThisThread::suspend(&mut self.context);
        if let Some(msg) = self.panicking {
            panic::panic_any(msg);
        }
    }
}
