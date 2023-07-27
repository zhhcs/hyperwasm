use std::{process::exit, sync::Arc};

use hyper_scheduler::{
    runtime::Runtime,
    runwasm::{run_wasm, Config},
};

fn main() {
    let rt = Arc::new(Runtime::new());

    let config = Config::new(
        "/home/ubuntu/dev/hyper-scheduler/examples/add.wat",
        0,
        0,
        "add",
    );

    let _ = run_wasm(&rt, config);

    std::thread::sleep(std::time::Duration::from_millis(2_000));
    rt.print_completed_status();
    exit(0);
}
