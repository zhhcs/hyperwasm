use std::{collections::BinaryHeap, process::exit, ptr, sync::Arc};

use hyper_scheduler::runtime::Runtime;
pub use hyper_scheduler::task::{stack::StackSize, Coroutine};

fn main() {
    let rt: Arc<Runtime> = Arc::new(Runtime::new());
    let mut queue = BinaryHeap::new();
    for i in 0..5 {
        let co = Coroutine::new(
            Box::new(move || loop {}),
            StackSize::default(),
            true,
            Some(std::time::Duration::from_millis(100)),
            Some(std::time::Duration::from_millis(
                100 * (5 - i) * ((i + 1) % 2 + 1),
            )),
        );
        queue.push(ptr::NonNull::from(Box::leak(Box::new(*co))));
    }
    while let Some(co) = queue.pop() {
        println!("{}", unsafe { co.as_ref() }.get_schedulestatus());
    }
    drop(rt);
    exit(0)
}
