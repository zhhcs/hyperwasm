use hyper_scheduler::runtime::Runtime;
pub use hyper_scheduler::task::stack::StackSize;
use std::{process::exit, sync::Arc};

static NUM: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn main() {
    let rt = Arc::new(Runtime::new());

    rt.spawn(
        Box::new(move || {
            do_some_sub(1);
        }),
        Some(std::time::Duration::from_millis(100)),
        Some(std::time::Duration::from_millis(600)),
    );
    std::thread::sleep(std::time::Duration::from_millis(30));

    rt.spawn(
        Box::new(move || {
            do_some_add(2);
        }),
        Some(std::time::Duration::from_millis(100)),
        Some(std::time::Duration::from_millis(200)),
    );
    std::thread::sleep(std::time::Duration::from_millis(30));

    rt.spawn(
        Box::new(move || {
            do_some_sub(3);
        }),
        Some(std::time::Duration::from_millis(100)),
        Some(std::time::Duration::from_millis(400)),
    );
    std::thread::sleep(std::time::Duration::from_millis(30));

    rt.spawn(
        Box::new(move || {
            do_some_add(2);
        }),
        Some(std::time::Duration::from_millis(100)),
        Some(std::time::Duration::from_millis(300)),
    );
    std::thread::sleep(std::time::Duration::from_millis(30));

    std::thread::sleep(std::time::Duration::from_millis(2_000));
    assert_eq!(crate::NUM.load(std::sync::atomic::Ordering::Acquire), 0);
    rt.print_completed_status();
    exit(0);
}

fn do_some_add(i: i32) {
    for _ in 0..5_000_000 {
        NUM.fetch_add(i, std::sync::atomic::Ordering::Acquire);
    }
}
fn do_some_sub(i: i32) {
    for _ in 0..5_000_000 {
        NUM.fetch_sub(i, std::sync::atomic::Ordering::Acquire);
    }
}
