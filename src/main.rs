use std::{
    mem, ptr,
    sync::atomic::{AtomicU32, Ordering},
};

extern "C" {
    fn sigfillset(set: *mut libc::sigset_t) -> libc::c_int;
    fn sigdelset(set: *mut libc::sigset_t, signum: libc::c_int) -> libc::c_int;
    fn getcontext(ucp: *mut libc::ucontext_t) -> libc::c_int;
    fn setcontext(ucp: *const libc::ucontext_t) -> libc::c_int;
    fn swapcontext(oucp: *mut libc::ucontext_t, ucp: *const libc::ucontext_t) -> libc::c_int;
    fn makecontext(
        ucp: *mut libc::ucontext_t,
        func: extern "C" fn(*mut libc::c_void),
        argc: libc::c_int,
        ...
    );
}

pub const CO_STACK_SIZE: usize = 128 * 1024;
pub const CO_EXIT_STACK_SIZE: usize = 4096;
pub static mut ID: AtomicU32 = AtomicU32::new(0);
pub static mut NUM_TASK_RUNNING: AtomicU32 = AtomicU32::new(0);
pub static mut TASKS: Vec<Coroutine> = Vec::new();
pub static mut EXITS: Vec<Coroutine> = Vec::new();
pub const CO_NOTIFY_SIGNO: libc::c_int = libc::SIGURG;

pub fn get_id() -> u32 {
    unsafe { ID.fetch_add(1, Ordering::SeqCst) }
}

pub fn add_task() -> u32 {
    unsafe { NUM_TASK_RUNNING.fetch_add(1, Ordering::SeqCst) }
}

pub fn remove_task() -> u32 {
    unsafe { NUM_TASK_RUNNING.fetch_sub(1, Ordering::SeqCst) }
}

pub struct Context {
    stack: *mut libc::c_void,
    context: libc::ucontext_t,
}

#[derive(Debug)]
pub struct Entry {
    pub f: extern "C" fn(*mut libc::c_void),
    pub arg: *mut libc::c_void,
    pub stack_size: usize,
}

unsafe impl Sync for Context {}

impl Context {
    pub fn empty() -> Context {
        unsafe { mem::zeroed() }
    }

    pub fn new(entry: &Entry, returns: Option<&mut Context>, stack_size: usize) -> Box<Context> {
        let stack = unsafe { libc::malloc(stack_size) };
        let mut ctx = Box::new(Context::empty());
        ctx.stack = stack;

        let rc = unsafe { getcontext(&mut ctx.context) };
        if rc != 0 {
            panic!("getcontext returns {}", rc);
        }

        ctx.context.uc_stack.ss_flags = 0;
        unsafe {
            sigfillset(&mut ctx.context.uc_sigmask);
            sigdelset(&mut ctx.context.uc_sigmask, CO_NOTIFY_SIGNO);
        };
        ctx.context.uc_link = match returns {
            Option::None => ptr::null_mut(),
            Option::Some(context) => &mut context.context,
        };

        ctx.context.uc_stack.ss_flags = 0;
        ctx.context.uc_stack.ss_size = stack_size;
        ctx.context.uc_stack.ss_sp = stack;

        if ctx.context.uc_link != ptr::null_mut() {
            unsafe { makecontext(&mut ctx.context, entry.f, 1, entry.arg) };
        } else {
            unsafe { makecontext(&mut ctx.context, entry.f, 0) };
        }
        ctx
    }

    pub fn resume(&self) {
        let rc = unsafe { setcontext(&self.context) };
        if rc != 0 {
            panic!("setcontext returns {}", rc);
        }
    }

    pub fn switch(&self, backup: &mut Context) {
        let rc = unsafe { swapcontext(&mut backup.context, &self.context) };
        if rc != 0 {
            panic!("swapcontext returns {}", rc);
        }
    }
}

#[derive(Clone, Copy)]
pub enum CoState {
    // 仅有两个状态, running和exit
    // exit状态代表此协程主动请求退出
    CoStateRunning,
    CoStateExit,
}

pub struct Coroutine {
    // 单个协程的id
    co_id: u32,
    // 单个协程的上下文
    co_ctx: Context,
    // 退出协程函数的上下文
    co_exit_ctx: Context,
    // 协程当前状态
    co_state: CoState,

    f: Option<Box<dyn FnOnce()>>,
}

impl Coroutine {
    pub fn new(f: Box<dyn FnOnce()>) -> Box<Coroutine> {
        let mut co = Box::new(Coroutine {
            f: Option::Some(f),
            co_id: get_id(),
            co_ctx: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            co_exit_ctx: unsafe { mem::MaybeUninit::zeroed().assume_init() },
            co_state: CoState::CoStateRunning,
        });
        add_task();

        let exit = Entry {
            f: Self::co_exit,
            arg: (co.as_mut() as *mut Coroutine) as *mut libc::c_void,
            stack_size: CO_EXIT_STACK_SIZE,
        };

        mem::forget(mem::replace(
            &mut co.co_exit_ctx,
            *Context::new(&exit, None, CO_EXIT_STACK_SIZE),
        ));

        let entry = Entry {
            f: Self::main,
            arg: (co.as_mut() as *mut Coroutine) as *mut libc::c_void,
            stack_size: CO_STACK_SIZE,
        };
        mem::forget(mem::replace(
            &mut co.co_ctx,
            *Context::new(&entry, Some(&mut co.co_exit_ctx), CO_STACK_SIZE),
        ));
        co
    }

    pub fn get_co_id(&self) -> u32 {
        self.co_id
    }

    pub fn get_co_state(&self) -> CoState {
        self.co_state
    }

    extern "C" fn main(arg: *mut libc::c_void) {
        let co = unsafe { &mut *(arg as *mut Coroutine) };
        co.run();
    }

    fn run(&mut self) {
        let f = self.f.take().expect("no entry function");
        f();
    }

    extern "C" fn co_exit(_arg: *mut libc::c_void) {
        let co_sche = unsafe { CO_SCHE as *mut CoSchedule };
        unsafe {
            if (*co_sche).cur_running_task == 0 {
                libc::exit(0);
            }
        }
        let mut newmask: libc::sigset_t = unsafe { mem::MaybeUninit::zeroed().assume_init() };
        let mut oldmask: libc::sigset_t = unsafe { mem::MaybeUninit::zeroed().assume_init() };
        unsafe {
            sigfillset(&mut newmask);
            libc::pthread_sigmask(libc::SIG_SETMASK, &newmask, &mut oldmask);

            libc::pthread_mutex_lock(&mut (*co_sche).mutex);
            // (*(co_sche)).cur_running_task.co_state = CoState::CoStateExit;
            libc::pthread_mutex_unlock(&mut (*co_sche).mutex);
            libc::pthread_sigmask(libc::SIG_SETMASK, &oldmask, std::ptr::null_mut());
        }
        loop {}
    }
}

pub struct CoSchedule {
    pub task_main: Coroutine,
    pub cur_running_task: u32,
    // pub n_task_running: u32,
    // pub id_inc: u32,
    pub mutex: libc::pthread_mutex_t,
}

pub static mut CO_SCHE: usize = 0;

impl CoSchedule {
    pub fn new(f: Box<dyn FnOnce()>) -> Self {
        CoSchedule {
            task_main: *Coroutine::new(f),
            cur_running_task: 0,
            mutex: unsafe { mem::MaybeUninit::zeroed().assume_init() },
        }
    }

    pub fn init(&self) {
        unsafe {
            let c_ptr: *const CoSchedule = self;
            CO_SCHE = c_ptr as usize;
        }
    }

    pub fn spawn(&mut self, f: Box<dyn FnOnce()>) {
        let mut newmask: libc::sigset_t = unsafe { mem::MaybeUninit::zeroed().assume_init() };
        let mut oldmask: libc::sigset_t = unsafe { mem::MaybeUninit::zeroed().assume_init() };
        unsafe {
            sigfillset(&mut newmask);
            libc::pthread_sigmask(libc::SIG_SETMASK, &newmask, &mut oldmask);

            libc::pthread_mutex_lock(&mut self.mutex)
        };

        let new_co = Coroutine::new(f);

        unsafe {
            TASKS.push(*new_co);
            libc::pthread_mutex_unlock(&mut self.mutex);

            libc::pthread_sigmask(libc::SIG_SETMASK, &oldmask, std::ptr::null_mut());
        };
    }
}

pub fn run(f: Box<dyn FnOnce()>) {
    let mut co_sche = CoSchedule::new(f);
    co_sche.init();
    unsafe { libc::pthread_mutex_init(&mut co_sche.mutex, std::ptr::null()) };
    let mut tid: libc::pthread_t = unsafe { mem::MaybeUninit::zeroed().assume_init() };
    let mut attr: libc::pthread_attr_t = unsafe { mem::MaybeUninit::zeroed().assume_init() };

    unsafe {
        libc::pthread_attr_init(&mut attr);
        libc::pthread_create(&mut tid, &attr, main_thread_setup, core::ptr::null_mut())
    };
    loop {}
}

// 在进入这个线程时, 它的所有信号都是屏蔽的
extern "C" fn main_thread_setup(arg: *mut libc::c_void) -> *mut libc::c_void {
    // 设置信号处理函数, 在信号处理函数中阻塞所有信号
    let mut action = libc::sigaction {
        sa_sigaction: co_signal_handler as libc::sighandler_t,
        sa_mask: unsafe { std::mem::zeroed() },
        sa_flags: libc::SA_SIGINFO | libc::SA_RESTART,
        sa_restorer: None,
    };
    unsafe { sigfillset(&mut action.sa_mask) };
    unsafe { libc::sigaction(CO_NOTIFY_SIGNO, &action, std::ptr::null_mut()) };

    // 现在可以处理信号了, 将接收调度信号, setcontext里面已经设置好
    // 了sigmask
    let co_sche = unsafe { CO_SCHE as *mut CoSchedule };
    unsafe { setcontext(&mut (*co_sche).task_main.co_ctx.context) };
    // unreachable
    std::ptr::null_mut()
}

fn co_signal_handler() {
    // unsafe { libc::pthread_mutex_lock(&mutex) };
}
fn main() {
    run(Box::new(move || println!("Hello world")));
}
