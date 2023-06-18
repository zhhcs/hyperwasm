use std::{
    collections::LinkedList,
    mem::MaybeUninit,
    ptr::null,
    sync::{Arc, Mutex},
};

use lazy_static::lazy_static;

pub const CO_STACK_SIZE: usize = 128 * 1024;
pub const CO_EXIT_STACK_SIZE: usize = 4096;

#[derive(Clone, Debug)]
pub enum CoStateT {
    CoStateRunning,
    CoStateExit,
}

#[derive(Clone, Debug)]
pub struct CoTaskNode {
    pub co_id: u32,
    pub co_ctx: libc::ucontext_t,
    pub co_exit_ctx: libc::ucontext_t,
    pub co_state: CoStateT,
    pub co_alloc_stack: *mut libc::c_void,
    pub exit_alloc_stack: *mut libc::c_void,
}

// unsafe impl Sync for CoTaskNode {}
// unsafe impl Send for CoTaskNode {}
impl CoTaskNode {
    // pub fn new() -> Self {
    //     let mut co_sche = CO_SCHE.try_lock().unwrap();
    //     co_sche.id_inc += 1;
    //     co_sche.n_task_running += 1;
    //     let co_id = co_sche.id_inc;
    //     let co_ctx: libc::ucontext_t = unsafe { MaybeUninit::zeroed().assume_init() };
    //     let co_exit_ctx: libc::ucontext_t = unsafe { MaybeUninit::zeroed().assume_init() };
    //     CoTaskNode {
    //         node: LinkedList::new(),
    //         co_id,
    //         co_ctx,
    //         co_exit_ctx,
    //         co_state: CoStateT::CoStateRunning,
    //         co_alloc_stack: unsafe { libc::malloc(128 * 1024) },
    //         exit_alloc_stack: unsafe { libc::malloc(4096) },
    //     }
    // }
}

#[derive(Clone, Debug)]
pub struct CoSchedule {
    pub task_main: Arc<Mutex<CoTaskNode>>,
    pub cur_running_task: Arc<Mutex<CoTaskNode>>,
    pub n_task_running: u32,
    pub id_inc: u32,
}

impl CoSchedule {
    pub fn sched_init() -> Self {
        let main_stack = unsafe { libc::malloc(CO_STACK_SIZE) };
        let main_exit_stack = unsafe { libc::malloc(CO_EXIT_STACK_SIZE) };
        let task_main: CoTaskNode = CoTaskNode {
            co_id: 0,
            co_ctx: unsafe { MaybeUninit::zeroed().assume_init() },
            co_exit_ctx: unsafe { MaybeUninit::zeroed().assume_init() },
            co_state: CoStateT::CoStateRunning,
            co_alloc_stack: main_stack,
            exit_alloc_stack: main_exit_stack,
        };
        let arc = Arc::new(Mutex::new(task_main));

        let co_sche = CoSchedule {
            task_main: arc.clone(),
            cur_running_task: arc.clone(),
            n_task_running: 1,
            id_inc: 1,
        };

        let mut task = co_sche.task_main.lock().unwrap();
        unsafe {
            libc::getcontext(&mut task.co_exit_ctx);
            libc::sigfillset(&mut task.co_exit_ctx.uc_sigmask);
            libc::sigdelset(&mut task.co_exit_ctx.uc_sigmask, libc::SIGURG);
        }
        task.co_exit_ctx.uc_stack.ss_flags = 0;
        task.co_exit_ctx.uc_stack.ss_size = CO_EXIT_STACK_SIZE;
        task.co_exit_ctx.uc_stack.ss_sp = main_exit_stack;
        unsafe { libc::makecontext(&mut task.co_exit_ctx, libc::exit(0), 0) };
        unsafe {
            libc::getcontext(&mut task.co_ctx);
            libc::sigfillset(&mut task.co_exit_ctx.uc_sigmask);
            libc::sigdelset(&mut task.co_exit_ctx.uc_sigmask, libc::SIGURG)
        };
        task.co_ctx.uc_link = &mut task.co_exit_ctx;
        task.co_ctx.uc_stack.ss_flags = 0;
        task.co_ctx.uc_stack.ss_size = CO_STACK_SIZE;
        task.co_ctx.uc_stack.ss_sp = main_stack;
        co_sche
    }
}
// lazy_static! {
//     pub static ref CO_TASK_0: CoTaskNode = CoTaskNode {
//         node: LinkedList::new(),
//         co_id: 0,
//         co_ctx: unsafe { MaybeUninit::zeroed().assume_init() },
//         co_exit_ctx: unsafe { MaybeUninit::zeroed().assume_init() },
//         co_state: CoStateT::CoStateRunning,
//         co_alloc_stack: unsafe { MaybeUninit::zeroed().assume_init() },
//         exit_alloc_stack: unsafe { MaybeUninit::zeroed().assume_init() },
//     };
//     pub static ref CO_SCHE: Mutex<CoSchedule<'static>> = Mutex::new(CoSchedule {
//         task_main: &CO_TASK_0,
//         cur_running_task: &CO_TASK_0,
//         n_task_running: 0,
//         id_inc: 0,
//     });
// }
