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

    pub fn print_status(&self) {
        let s = self.scheduler.get_status().unwrap();
        s.iter().for_each(|(id, stat)| {
            println!("id: {}, status: \n{}", id, stat);
        });
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        while let Some(t) = self.threads.pop() {
            t.join().unwrap();
        }
    }
}
