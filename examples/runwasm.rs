use std::{process::exit, sync::Arc};

use hyper_scheduler::runtime::Runtime;
use wasmtime::*;

fn main() {
    let rt = Arc::new(Runtime::new());
    rt.spawn(
        move || {
            do_some_sub(1);
        },
        Some(std::time::Duration::from_millis(100)),
        Some(std::time::Duration::from_millis(200)),
    );
    std::thread::sleep(std::time::Duration::from_millis(30));
    let _ = run_wasm(&rt);
    rt.spawn(
        move || {
            do_some_add(1);
        },
        Some(std::time::Duration::from_millis(100)),
        Some(std::time::Duration::from_millis(300)),
    );

    std::thread::sleep(std::time::Duration::from_millis(2_000));
    assert_eq!(crate::NUM.load(std::sync::atomic::Ordering::Acquire), 0);

    rt.print_completed_status();
    exit(0);
}

fn run_wasm(rt: &Runtime) -> wasmtime::Result<()> {
    let engine = Engine::default();
    let wat = r#"
        (module
            (import "host" "host_func" (func $host_hello (param i32)))

            (func (export "hello")
                i32.const 3
                call $host_hello)
        )
    "#;
    let module = Module::new(&engine, wat)?;

    // Create a `Linker` and define our host function in it:
    let mut linker = Linker::new(&engine);
    linker.func_wrap(
        "host",
        "host_func",
        |caller: Caller<'_, u32>, param: i32| {
            println!("Got {} from WebAssembly", param);
            println!("my host state is: {}", caller.data());
        },
    )?;

    // Use the `linker` to instantiate the module, which will automatically
    // resolve the imports of the module using name-based resolution.
    let mut store = Store::new(&engine, 0);
    let instance = linker.instantiate(&mut store, &module)?;
    let hello = instance.get_typed_func::<(), ()>(&mut store, "hello")?;
    let func = move || {
        if let Ok(()) = hello.call(&mut store, ()) {};
    };
    rt.spawn(
        func,
        Some(std::time::Duration::from_micros(500)),
        Some(std::time::Duration::from_millis(1)),
    );

    Ok(())
}

static NUM: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

fn do_some_sub(i: i32) {
    for _ in 0..5_000_000 {
        NUM.fetch_sub(i, std::sync::atomic::Ordering::Acquire);
    }
}

fn do_some_add(i: i32) {
    for _ in 0..5_000_000 {
        NUM.fetch_add(i, std::sync::atomic::Ordering::Acquire);
    }
}
