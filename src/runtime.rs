use crate::{scheduler::Scheduler, task::Coroutine, StackSize};
use std::{sync::Arc, thread::JoinHandle};

pub(crate) struct Runtime {
    scheduler: Arc<Scheduler>,
    threads: Vec<JoinHandle<()>>,
}

impl Runtime {
    pub fn new() -> Runtime {
        let scheduler = Scheduler::new();
        let threads = Scheduler::start(&scheduler);
        Runtime { scheduler, threads }
    }

    pub fn spawn(&self, f: Box<dyn FnOnce()>) -> Result<(), std::io::Error> {
        let co = Coroutine::new(Box::new(move || f()), StackSize::default(), false);
        self.scheduler.push(co)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        while let Some(t) = self.threads.pop() {
            t.join().unwrap();
        }
    }
}
