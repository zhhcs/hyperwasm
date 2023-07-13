pub mod cgroupv2;
pub mod runtime;
pub mod scheduler;
pub mod task;
use runtime::Runtime;
use std::{process::exit, sync::Arc};
pub use task::stack::StackSize;

static NUM: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn main() {
    let rt = Arc::new(Runtime::new());
    for i in 1..6 {
        rt.spawn(Box::new(move || {
            let i = i.clone();
            do_some_sub(i);
        }))
        .unwrap();
    }
    for i in 1..6 {
        rt.spawn(Box::new(move || {
            let i = i.clone();
            do_some_add(i);
        }))
        .unwrap();
    }

    std::thread::sleep(std::time::Duration::from_millis(10_000));
    assert_eq!(crate::NUM.load(std::sync::atomic::Ordering::Acquire), 0);
    exit(0);
}

fn do_some_add(i: i32) {
    for _ in 0..1_000_000 {
        NUM.fetch_add(i, std::sync::atomic::Ordering::Acquire);
    }
}
fn do_some_sub(i: i32) {
    for _ in 0..1_000_000 {
        NUM.fetch_sub(i, std::sync::atomic::Ordering::Acquire);
    }
}
