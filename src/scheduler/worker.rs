use super::{get_timer, Scheduler};
use crate::{
    task::{current, current_is_none, CoStatus, Coroutine, SchedulerStatus},
    StackSize,
};
use nix::unistd::Pid;
use std::{
    cell::Cell,
    collections::{BinaryHeap, HashMap, VecDeque},
    ptr,
    sync::Arc,
};

thread_local! {
    static WORKER: Cell<Option<ptr::NonNull<Worker>>> = Cell::new(None);
}

pub fn get_worker() -> ptr::NonNull<Worker> {
    WORKER.with(|cell| cell.get()).expect("no worker")
}

pub type ArrayQueue<T> = VecDeque<T>;

pub struct Worker {
    worker_id: u8,
    new_spawned: ArrayQueue<Box<Coroutine>>,
    local_queue: ArrayQueue<ptr::NonNull<Coroutine>>,
    suspend_queue: ArrayQueue<ptr::NonNull<Coroutine>>,
    realtime_queue: HashMap<u64, ptr::NonNull<Coroutine>>,
    realtime_status: BinaryHeap<SchedulerStatus>,
    scheduler: Arc<Scheduler>,
    curr: Option<ptr::NonNull<Coroutine>>,
    capacity: usize,
    pub len: usize,
    // signal
}

unsafe impl Send for Worker {}
unsafe impl Sync for Worker {}

impl Worker {
    pub fn new(scheduler: &Arc<Scheduler>, capacity: usize, worker_id: u8) -> Arc<Worker> {
        let new_spawned = ArrayQueue::with_capacity(capacity);
        let local_queue = ArrayQueue::with_capacity(capacity);
        let suspend_queue = ArrayQueue::with_capacity(capacity);
        let realtime_queue = HashMap::with_capacity(capacity);
        let realtime_status = BinaryHeap::with_capacity(capacity);

        Arc::new(Worker {
            worker_id,
            new_spawned,
            local_queue,
            suspend_queue,
            realtime_queue,
            realtime_status,
            scheduler: scheduler.clone(),
            curr: None,
            capacity,
            len: 0,
        })
    }

    pub fn init(&self) {
        let worker = ptr::NonNull::from(self);
        WORKER.with(|t| t.set(Some(worker)));
    }

    pub fn set_cgroup(&self, tid: Pid) {
        let cg_worker = crate::cgroupv2::Controllerv2::new(
            std::path::PathBuf::from("/sys/fs/cgroup/hypersched"),
            format!("worker{}", tid),
        );
        cg_worker.set_threaded();
        cg_worker.set_cpuset(self.worker_id + 2, None);
        cg_worker.set_cgroup_threads(tid);
    }

    pub fn set_curr(&mut self) {
        if let Some(co) = self.take_realtime() {
            // tracing::info!("now setting current {:?}", std::time::Instant::now());
            self.curr = Some(co);
        } else if let Some(co) = self.local_queue.pop_front() {
            self.curr = Some(co);
        } else if let Some(co) = self.new_spawned.pop_front() {
            let co = ptr::NonNull::from(Box::leak(Box::new(*co)));
            self.curr = Some(co);
        } else if let Some(co) = self.suspend_queue.pop_front() {
            self.curr = Some(co);
        }
    }

    pub fn preemptive(&mut self) -> bool {
        while let Some(co) = self.scheduler.get_slots(self.worker_id) {
            // tracing::info!("{} preempt", co.get_co_id());
            self.curr = Some(ptr::NonNull::from(Box::leak(Box::new(*co))));
            self.len += 1;
            return true;
        }
        false
    }

    pub fn add_realtime(&mut self, co: Box<Coroutine>) {
        self.realtime_status.push(co.get_schedulestatus());
        self.realtime_queue
            .insert(co.get_co_id(), ptr::NonNull::from(Box::leak(Box::new(*co))));
    }

    pub fn take_realtime(&mut self) -> Option<ptr::NonNull<Coroutine>> {
        if let Some(stat) = self.realtime_status.pop() {
            return Some(
                self.realtime_queue
                    .remove_entry(&stat.get_co_id())
                    .unwrap()
                    .1,
            );
        }
        None
    }

    pub fn get_task(&mut self) {
        while let Some(co) = self.scheduler.pop_realtime(self.worker_id) {
            // tracing::info!("now getting task {:?}", std::time::Instant::now());
            self.add_realtime(co);
            self.len += 1;
        }
        while !self.is_full() && self.scheduler.get_length() > 0 {
            if let Some(co) = self.scheduler.pop() {
                // tracing::info!("get coroutine co id = {} from global queue", co.get_co_id());
                self.new_spawned.push_back(co);
                self.len += 1;
            }
        }
    }

    fn is_full(&self) -> bool {
        self.len >= self.capacity
    }

    pub fn run(&mut self) {
        loop {
            if current_is_none() {
                if let Some(mut co) = self.curr.take() {
                    let co = unsafe { co.as_mut() };
                    if co.get_status() == CoStatus::PENDING {
                        co.init();
                    }
                    // let id = co.get_co_id();
                    // tracing::info!(
                    //     "now {:?} co id = {} is ready to run",
                    //     std::time::Instant::now(),
                    //     id
                    // );
                    self.run_co(co.into(), self.worker_id);
                } else {
                    if self.len < self.capacity / 2 {
                        self.get_task();
                    }
                    self.set_curr();
                }
            }
        }
    }

    pub fn suspend(&mut self) {
        if let Some(mut curr) = current() {
            let curr = unsafe { curr.as_mut() };
            if curr.get_status() != CoStatus::COMPLETED {
                curr.set_status(CoStatus::SUSPENDED);
            }
            if curr.is_realtime() {
                self.realtime_status.push(curr.get_schedulestatus());
                self.realtime_queue.insert(curr.get_co_id(), curr.into());
            } else {
                self.suspend_queue.push_back(curr.into());
            }
            curr.suspend(&self.scheduler, self.worker_id);
        }
    }

    // TODO: spawn local
    pub fn _spawn_local(&mut self, f: Box<dyn FnOnce()>) {
        let co = Coroutine::new(f, StackSize::default(), true, None, None);
        let co = ptr::NonNull::from(Box::leak(Box::new(*co)));
        self.local_queue.push_back(co);
        self.len += 1;
        tracing::info!("spawning local coroutine");
    }

    fn run_co(&mut self, mut co: ptr::NonNull<Coroutine>, worker_id: u8) {
        // tracing::info!("running coroutine");
        unsafe { get_timer().as_mut().reset_timer() };

        let c = unsafe { co.as_mut() };
        if c.resume(&self.scheduler, self.worker_id) {
            return;
        }
        self.len -= 1;
        self.scheduler
            .update_completed_status(c.get_co_id(), c.get_schedulestatus(), worker_id);
        Self::drop_coroutine(co);
    }

    fn drop_coroutine(co: ptr::NonNull<Coroutine>) {
        // tracing::info!("dropping coroutine");
        drop(unsafe { Box::from_raw(co.as_ptr()) });
        // unsafe { get_timer().as_mut().reset_timer() };
    }
}
