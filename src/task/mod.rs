mod context;
mod page_size;
pub(crate) mod stack;

use std::cell::{Cell, UnsafeCell};
use std::panic;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::{mem, ptr};

use self::context::{Context, Entry};
use self::stack::StackSize;

pub static mut ID: AtomicUsize = AtomicUsize::new(1);

fn get_id() -> usize {
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

pub enum CoStatus {
    PENDING = 1,
    RUNNING,
    COMPLETED,
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

pub(crate) struct Coroutine {
    context: Box<Context>,
    status: CoStatus,
    panicking: Option<&'static str>,
    f: Option<Box<dyn FnOnce()>>,
    id: usize,
}

unsafe impl Sync for Coroutine {}
unsafe impl Send for Coroutine {}

impl Coroutine {
    pub fn new(f: Box<dyn FnOnce()>, stack_size: StackSize) -> Box<Coroutine> {
        #[allow(invalid_value)]
        let mut co = Box::new(Coroutine {
            f: Option::Some(f),
            context: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            status: CoStatus::PENDING,
            panicking: None,
            id: get_id(),
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
        co.status = CoStatus::COMPLETED;
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
    pub fn resume(&mut self) -> bool {
        // println!("start resume");

        let _scope = Scope::enter(self);

        ThisThread::resume(&self.context);

        match self.status {
            CoStatus::COMPLETED => false,
            _ => true,
        }
    }

    pub fn suspend(&mut self) {
        // println!("start suspend");
        ThisThread::suspend(&mut self.context);
        if let Some(msg) = self.panicking {
            panic::panic_any(msg);
        }
    }

    pub fn get_co_id(&self) -> usize {
        self.id
    }
}
