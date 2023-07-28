use std::{
    collections::{BinaryHeap, HashMap},
    process::exit,
    ptr,
    sync::Arc,
};

use hyper_scheduler::runtime::Runtime;
pub use hyper_scheduler::task::{stack::StackSize, Coroutine};

fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");
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
        tracing::info!("{}", unsafe { co.as_ref() }.get_schedulestatus());
    }

    let mut map = HashMap::new();
    let mut heap = BinaryHeap::new();
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
        heap.push(co.get_schedulestatus());
        map.insert(co.get_co_id(), ptr::NonNull::from(Box::leak(Box::new(*co))));
    }
    while let Some(co) = heap.pop() {
        tracing::info!(
            "{}",
            unsafe { map.get(&co.get_co_id()).unwrap().as_ref() }.get_schedulestatus()
        );
    }
    drop(rt);
    exit(0)
}
