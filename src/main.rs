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
    for i in 1..3 {
        rt.spawn(Box::new(move || {
            let i = i.clone();
            if i % 2 == 0 {
                do_some_sub(i);
            } else {
                do_some_add(i);
            }
            println!(
                "NUM {}",
                crate::NUM.load(std::sync::atomic::Ordering::Acquire)
            );
        }))
        .unwrap();
    }
    for i in 1..3 {
        rt.spawn(Box::new(move || {
            let i = i.clone();
            if i % 2 == 0 {
                do_some_add(i);
            } else {
                do_some_sub(i);
            }
            println!(
                "NUM {}",
                crate::NUM.load(std::sync::atomic::Ordering::Acquire)
            );
        }))
        .unwrap();
    }

    std::thread::sleep(std::time::Duration::from_millis(10_000));
    assert_eq!(crate::NUM.load(std::sync::atomic::Ordering::Acquire), 0);
    rt.print_status();
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
