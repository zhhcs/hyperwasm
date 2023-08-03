use std::thread;
use std::time::Duration;

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
    let config = Config::new(
        "hello",
        "/home/ubuntu/dev/hyper-scheduler/examples/test.wasm",
        0,
        0,
        "fib",
    );
    client.init(&config).await.unwrap();
    thread::sleep(Duration::from_millis(2000));
    for i in 0..2 {
        let mut name = String::from("task");
        name.push_str(&i.to_string());
        let config = Config::new(
            &name,
            "/home/ubuntu/dev/hyper-scheduler/examples/test.wasm",
            0,
            0,
            "fib",
        );
        client
            .call(&config, "http://127.0.0.1:3001/fib")
            .await
            .unwrap();
    }
    client.get_status().await.unwrap();
}
