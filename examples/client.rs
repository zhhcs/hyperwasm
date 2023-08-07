// use std::thread;
// use std::time::Duration;

use hyper_scheduler::axum::client::Client;
use hyper_scheduler::runwasm::Config;

// cargo build --release --package hyper-scheduler --example client
// sudo ./target/release/examples/client
#[tokio::main]
async fn main() {
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    tracing::info!("Starting");
    let client = Client::new();

    let eet33 = 15;
    fib(&client, "task1", 0, 0, 40).await;
    // fib33(&client, "task2", eet33, 70).await;
    // thread::sleep(Duration::from_millis(5));
    // fib(&client, "task3", 0, 0, 33).await;
    // fib33(&client, "task4", 0, 0).await;
    fib(&client, "task5", 10, 25, 32).await;
    fib33(&client, "task6", 0, 0).await;
    fib(&client, "task7", 8, 40, 31).await;
    // thread::sleep(Duration::from_millis(5));
    fib33(&client, "task8", eet33, 60).await;
    fib(&client, "task9", 0, 0, 34).await;
}

async fn fib(client: &Client, name: &str, eet: u64, ddl: u64, param: i32) {
    let config = Config::new(
        &name,
        "/home/ubuntu/dev/hyper-scheduler/examples/fib().wasm",
        eet,
        ddl,
        "fib",
        Some("fib".to_owned()),
        Some(param),
    );
    client
        .call(&config, "http://127.0.0.1:3001/fib")
        .await
        .unwrap();
}

async fn fib33(client: &Client, name: &str, eet: u64, ddl: u64) {
    let config = Config::new(
        &name,
        "/home/ubuntu/dev/hyper-scheduler/examples/fib33.wasm",
        eet,
        ddl,
        "fib33",
        Some("fib33".to_owned()),
        None,
    );
    client
        .call(&config, "http://127.0.0.1:3002/fib33")
        .await
        .unwrap();
}
