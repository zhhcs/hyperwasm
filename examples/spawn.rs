use hyper_scheduler::runtime::Runtime;
pub use hyper_scheduler::task::stack::StackSize;
use std::{process::exit, sync::Arc};

static NUM: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");
    let rt = Arc::new(Runtime::new());

    let _ = rt.spawn(
        move || {
            do_some_sub(1);
        },
        None,
        None,
    );
    std::thread::sleep(std::time::Duration::from_millis(10));

    let _ = rt.spawn(
        move || {
            do_some_sub(1);
        },
        Some(std::time::Duration::from_millis(35)),
        Some(std::time::Duration::from_millis(200)),
    );
    std::thread::sleep(std::time::Duration::from_millis(10));

    let _ = rt.spawn(
        move || {
            do_some_add(2);
        },
        Some(std::time::Duration::from_millis(35)),
        Some(std::time::Duration::from_millis(70)),
    );
    std::thread::sleep(std::time::Duration::from_millis(10));

    let _ = rt.spawn(
        move || {
            do_some_sub(3);
        },
        Some(std::time::Duration::from_millis(35)),
        Some(std::time::Duration::from_millis(130)),
    );
    std::thread::sleep(std::time::Duration::from_millis(10));

    let _ = rt.spawn(
        move || {
            do_some_add(1);
        },
        None,
        None,
    );
    std::thread::sleep(std::time::Duration::from_millis(10));

    let _ = rt.spawn(
        move || {
            do_some_add(2);
        },
        Some(std::time::Duration::from_millis(60)),
        Some(std::time::Duration::from_millis(70)),
    );
    std::thread::sleep(std::time::Duration::from_millis(10));

    let _ = rt.spawn(
        move || {
            do_some_add(2);
        },
        Some(std::time::Duration::from_millis(35)),
        Some(std::time::Duration::from_millis(100)),
    );
    std::thread::sleep(std::time::Duration::from_millis(110));

    let _ = rt.spawn(
        move || {
            do_some_add(0);
        },
        None,
        None,
    );
    std::thread::sleep(std::time::Duration::from_millis(2_000));
    // assert_eq!(crate::NUM.load(std::sync::atomic::Ordering::Acquire), 0);
    // rt.print_completed_status();
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
