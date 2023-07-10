use std::{cell::Cell, collections::VecDeque, ptr, sync::Arc};

// use crossbeam::queue::ArrayQueue;
pub(crate) type ArrayQueue<T> = VecDeque<T>;

use crate::{
    runtime::Runtime,
    task::{current, current_is_none, Coroutine},
    StackSize,
};

thread_local! {
    static WORKER: Cell<Option<ptr::NonNull<Worker>>> = Cell::new(None);
}

pub(crate) fn get_worker() -> ptr::NonNull<Worker> {
    WORKER.with(|cell| cell.get()).expect("no worker")
}

pub(crate) struct Worker {
    local_queue: ArrayQueue<ptr::NonNull<Coroutine>>,
    rt: Arc<Runtime>,
    curr: Option<ptr::NonNull<Coroutine>>,
    capacity: usize,
    len: usize,
    // signal
}

unsafe impl Send for Worker {}
unsafe impl Sync for Worker {}

impl Worker {
    pub fn new(rt: Arc<Runtime>, capacity: usize) -> Arc<Worker> {
        let local_queue = ArrayQueue::with_capacity(capacity);
        Arc::new(Worker {
            local_queue,
            rt,
            curr: None,
            capacity,
            len: 0,
        })
    }

    pub(crate) fn init(&self) {
        let worker = ptr::NonNull::from(self);
        WORKER.with(|t| t.set(Some(worker)));
    }

    pub(crate) fn set_curr(&mut self) {
        if current_is_none() {
            if let Some(co) = self.local_queue.pop_front() {
                self.curr = Some(co);
            }
        }
    }

    pub fn get_task(&mut self) {
        if !self.is_full() && self.rt.queue_len() > 0 {
            if let Some(co) = self.rt.take() {
                let mut co = ptr::NonNull::from(Box::leak(Box::new(*co)));
                unsafe { co.as_mut().init() };
                self.local_queue.push_back(co);
                self.len += 1;
            }
        }
    }

    fn is_full(&self) -> bool {
        self.len >= self.capacity
    }

    pub(crate) fn sched(&mut self) {
        println!("thread spawned");
        loop {
            if current_is_none() {
                self.get_task();

                if let Some(mut co) = self.curr.take() {
                    let id = unsafe { co.as_mut().get_co_id() };
                    println!("co id = {} is ready to run", id);
                    self.run_co(co);
                }
            }
        }
    }

    pub(crate) fn suspend(&mut self) {
        if let Some(mut curr) = current() {
            let curr = unsafe { curr.as_mut() };
            self.local_queue.push_back(curr.into());
            curr.suspend();
        }
    }

    pub fn spawn(&mut self, f: Box<dyn FnOnce()>) {
        let co = ptr::NonNull::from(Box::leak(Coroutine::new(f, StackSize::default(), true)));
        self.local_queue.push_back(co);
        self.len += 1;
        println!("spawning coroutine");
    }

    fn run_co(&mut self, mut co: ptr::NonNull<Coroutine>) {
        // println!("running coroutine");
        if unsafe { co.as_mut().resume() } {
            return;
        }
        Self::drop_coroutine(co);
    }

    fn drop_coroutine(co: ptr::NonNull<Coroutine>) {
        println!("dropping coroutine");
        drop(unsafe { Box::from_raw(co.as_ptr()) });
    }
}
