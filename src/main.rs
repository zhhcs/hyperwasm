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
        rt.spawn(
            Box::new(move || {
                let i = i.clone();
                if i % 2 == 0 {
                    do_some_sub();
                } else {
                    do_some_add();
                }
            }),
            None,
            None,
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    for i in 1..7 {
        rt.spawn(
            Box::new(move || {
                let i = i.clone();
                if i % 2 == 0 {
                    do_some_sub();
                } else {
                    do_some_add();
                }
            }),
            Some(std::time::Duration::from_millis(35)),
            Some(std::time::Duration::from_millis(rand::Rng::gen_range(
                &mut rand::thread_rng(),
                100..240,
            ))),
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    std::thread::sleep(std::time::Duration::from_millis(2_000));
    assert_eq!(crate::NUM.load(std::sync::atomic::Ordering::Acquire), 0);
    rt.print_status();
    exit(0);
}

fn do_some_add() {
    for _ in 0..5_000_000 {
        NUM.fetch_add(2, std::sync::atomic::Ordering::Acquire);
    }
}
fn do_some_sub() {
    for _ in 0..5_000_000 {
        NUM.fetch_sub(2, std::sync::atomic::Ordering::Acquire);
    }
}
