use std::{process::exit, sync::Arc};

use hyper_scheduler::runtime::Runtime;
use wasmtime::*;

fn main() {
    let rt = Arc::new(Runtime::new());

    let _ = run_wasm(&rt);
    // std::thread::sleep(std::time::Duration::from_millis(20));

    std::thread::sleep(std::time::Duration::from_millis(2_000));
    let num = NUM.load(std::sync::atomic::Ordering::Acquire);
    println!("num = {}", num);
    rt.print_completed_status();
    exit(0);
}

fn run_wasm(rt: &Runtime) -> wasmtime::Result<()> {
    // let start = std::time::Instant::now();
    let mut store = Store::<()>::default();
    let module = Module::from_file(store.engine(), "examples/add.wat")?;
    let instance = Instance::new(&mut store, &module, &[])?;

    let add = instance.get_typed_func::<(), i32>(&mut store, "add")?;

    let func = move || {
        if let Ok(res) = add.call(&mut store, ()) {
            NUM.fetch_add(res / 10000000, std::sync::atomic::Ordering::Relaxed);
        }
    };
    // let end = std::time::Instant::now();
    // println!("time: {:?}", end - start);
    rt.spawn(func, None, None);

    Ok(())
}

static NUM: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
