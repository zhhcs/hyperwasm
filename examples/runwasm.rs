use std::{process::exit, sync::Arc};

use hyper_scheduler::{
    runtime::Runtime,
    runwasm::{call, Config, Environment},
};

fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::TRACE)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");
    let rt = Arc::new(Runtime::new());

    let config1 = Config::new(
        "hello",
        "/home/ubuntu/dev/hyper-scheduler/examples/fib().wasm",
        0,
        0,
        "fib",
        None,
        None,
    );
    let env1 = Environment::new(&config1).unwrap();

    let config2 = Config::new(
        "hello",
        "/home/ubuntu/dev/hyper-scheduler/examples/fib33.wasm",
        0,
        0,
        "fib33",
        None,
        None,
    );
    let env2 = Environment::new(&config2).unwrap();
    // let eet33 = 15;
    fib(&env1, &rt, "task1", 0, 0, 40);
    // fib(&env1, &rt, "task5", 10, 25, 32);
    fib33(&env2, &rt, "task6", 0, 0);
    // fib(&env1, &rt, "task7", 8, 40, 31);
    // fib33(&env2, &rt, "task8", eet33, 60);
    fib(&env1, &rt, "task9", 0, 0, 34);
    std::thread::sleep(std::time::Duration::from_millis(2_000));
    // rt.print_completed_status();
    exit(0);
}

fn fib(env: &Environment, rt: &Runtime, name: &str, eet: u64, ddl: u64, param: i32) {
    let config = Config::new(
        &name,
        "/home/ubuntu/dev/hyper-scheduler/examples/fib().wasm",
        eet,
        ddl,
        "fib",
        Some("fib".to_owned()),
        Some(param),
    );

    let env = env.clone();
    let _ = call(&rt, env, Some(config));
}

fn fib33(env: &Environment, rt: &Runtime, name: &str, eet: u64, ddl: u64) {
    let config = Config::new(
        &name,
        "/home/ubuntu/dev/hyper-scheduler/examples/fib33.wasm",
        eet,
        ddl,
        "fib33",
        Some("fib33".to_owned()),
        None,
    );
    let env = env.clone();
    let _ = call(&rt, env, Some(config));
}
