use std::{
    collections::LinkedList,
    mem::MaybeUninit,
    sync::{Arc, Mutex},
};

use lazy_static::lazy_static;
enum CoStateT {
    CoStateRunning,
    CoStateExit,
}

pub struct CoTaskNode {
    node: LinkedList<usize>,
    co_id: u32,
    co_ctx: libc::ucontext_t,
    co_exit_ctx: libc::ucontext_t,
    co_state: CoStateT,
    co_alloc_stack: *mut libc::c_void,
    exit_alloc_stack: *mut libc::c_void,
}

unsafe impl Sync for CoTaskNode {}
unsafe impl Send for CoTaskNode {}
impl CoTaskNode {
    pub fn new() -> Self {
        let mut co_sche = CO_SCHE.try_lock().unwrap();
        co_sche.id_inc += 1;
        co_sche.n_task_running += 1;
        let co_id = co_sche.id_inc;
        let mut co_ctx: libc::ucontext_t = unsafe { MaybeUninit::zeroed().assume_init() };
        let mut co_exit_ctx: libc::ucontext_t = unsafe { MaybeUninit::zeroed().assume_init() };
        CoTaskNode {
            node: LinkedList::new(),
            co_id,
            co_ctx,
            co_exit_ctx,
            co_state: CoStateT::CoStateRunning,
            co_alloc_stack: unsafe { libc::malloc(128 * 1024) },
            exit_alloc_stack: unsafe { libc::malloc(4096) },
        }
    }
}

pub struct CoSchedule<'a> {
    task_main: &'a CoTaskNode,
    pub cur_running_task: &'a CoTaskNode,
    pub n_task_running: u32,
    pub id_inc: u32,
}

lazy_static! {
    static ref CO_TASK_0: CoTaskNode = CoTaskNode {
        node: LinkedList::new(),
        co_id: unsafe { MaybeUninit::zeroed().assume_init() },
        co_ctx: unsafe { MaybeUninit::zeroed().assume_init() },
        co_exit_ctx: todo!(),
        co_state: CoStateT::CoStateRunning,
        co_alloc_stack: unsafe { libc::malloc(128 * 1024) },
        exit_alloc_stack: unsafe { libc::malloc(4096) },
    };
    pub static ref CO_SCHE: Arc<Mutex<CoSchedule<'static>>> = Arc::new(Mutex::new(CoSchedule {
        task_main: &CO_TASK_0,
        cur_running_task: &CO_TASK_0,
        n_task_running: 0,
        id_inc: 0,
    }));
}
