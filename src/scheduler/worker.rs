use super::{get_timer, Scheduler};
use crate::{
    task::{current, current_is_none, CoStatus, Coroutine},
    StackSize,
};
use nix::unistd::Pid;
use std::{cell::Cell, collections::VecDeque, ptr, sync::Arc};

thread_local! {
    static WORKER: Cell<Option<ptr::NonNull<Worker>>> = Cell::new(None);
}

pub(crate) fn get_worker() -> ptr::NonNull<Worker> {
    WORKER.with(|cell| cell.get()).expect("no worker")
}

pub(crate) type ArrayQueue<T> = VecDeque<T>;

pub(crate) struct Worker {
    new_spawned: ArrayQueue<Box<Coroutine>>,
    local_queue: ArrayQueue<ptr::NonNull<Coroutine>>,
    suspend_queue: ArrayQueue<ptr::NonNull<Coroutine>>,
    scheduler: Arc<Scheduler>,
    curr: Option<ptr::NonNull<Coroutine>>,
    capacity: usize,
    pub len: usize,
    // signal
}

unsafe impl Send for Worker {}
unsafe impl Sync for Worker {}

impl Worker {
    pub(crate) fn new(scheduler: &Arc<Scheduler>, capacity: usize) -> Arc<Worker> {
        let new_spawned = ArrayQueue::with_capacity(capacity);
        let local_queue = ArrayQueue::with_capacity(capacity);
        let suspend_queue = ArrayQueue::with_capacity(capacity);
        Arc::new(Worker {
            new_spawned,
            local_queue,
            suspend_queue,
            scheduler: scheduler.clone(),
            curr: None,
            capacity,
            len: 0,
        })
    }

    pub(crate) fn init(&self) {
        let worker = ptr::NonNull::from(self);
        WORKER.with(|t| t.set(Some(worker)));
    }

    pub(crate) fn set_cgroup(&self, tid: Pid) {
        let cg_worker = crate::cgroupv2::Controllerv2::new(
            std::path::PathBuf::from("/sys/fs/cgroup/hypersched"),
            String::from("worker"),
        );
        cg_worker.set_threaded();
        cg_worker.set_cpuset(1, None);
        cg_worker.set_cgroup_threads(tid);
    }

    pub(crate) fn set_curr(&mut self) {
        if current_is_none() {
            if let Some(co) = self.local_queue.pop_front() {
                self.curr = Some(co);
            } else if let Some(co) = self.new_spawned.pop_front() {
                let co = ptr::NonNull::from(Box::leak(Box::new(*co)));
                self.curr = Some(co);
            } else if let Some(co) = self.suspend_queue.pop_front() {
                self.curr = Some(co);
            }
        }
    }

    pub(crate) fn get_task(&mut self) {
        while !self.is_full() && self.scheduler.get_length() > 0 {
            if let Some(co) = self.scheduler.pop() {
                // println!("get coroutine co id = {} from global queue", co.get_co_id());
                self.new_spawned.push_back(co);
                self.len += 1;
            }
        }
    }

    fn is_full(&self) -> bool {
        self.len >= self.capacity
    }

    pub(crate) fn run(&mut self) {
        loop {
            if current_is_none() {
                if let Some(mut co) = self.curr.take() {
                    let co = unsafe { co.as_mut() };
                    if co.get_status() == CoStatus::PENDING {
                        co.init();
                    }
                    // let id = co.get_co_id();
                    // println!("co id = {} is ready to run", id);
                    self.run_co(co.into());
                }
            }
        }
    }

    pub(crate) fn suspend(&mut self) {
        if let Some(mut curr) = current() {
            let curr = unsafe { curr.as_mut() };
            if curr.get_status() != CoStatus::COMPLETED {
                curr.set_status(CoStatus::SUSPENDED);
            }
            self.suspend_queue.push_back(curr.into());
            curr.suspend();
        }
    }

    pub fn spawn_local(&mut self, f: Box<dyn FnOnce()>) {
        let co = Coroutine::new(f, StackSize::default(), true);
        let co = ptr::NonNull::from(Box::leak(Box::new(*co)));
        self.local_queue.push_back(co);
        self.len += 1;
        println!("spawning local coroutine");
    }

    fn run_co(&mut self, mut co: ptr::NonNull<Coroutine>) {
        // println!("running coroutine");
        if unsafe { co.as_mut().resume() } {
            return;
        }
        self.len -= 1;
        Self::drop_coroutine(co);
    }

    fn drop_coroutine(co: ptr::NonNull<Coroutine>) {
        // println!("dropping coroutine");
        drop(unsafe { Box::from_raw(co.as_ptr()) });
        unsafe { get_timer().as_mut().reset_timer() };
    }
}
