use std::{process::exit, sync::Arc};

use hyper_scheduler::{
    runtime::Runtime,
    runwasm::{run_wasm, Config},
};

fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");
    let rt = Arc::new(Runtime::new());

    let config = Config::new(
        "task1",
        "/home/ubuntu/dev/hyper-scheduler/examples/add.wat",
        12,
        20,
        "add",
        None,
        None,
    );

    let _ = run_wasm(&rt, config);

    std::thread::sleep(std::time::Duration::from_millis(2_000));
    rt.print_completed_status();
    exit(0);
}
